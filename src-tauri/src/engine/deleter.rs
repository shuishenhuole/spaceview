//! 后台删除引擎:把"移到回收站"这一可能很慢的 I/O 并行放进工作线程。
//!
//! 与 UI 解耦:逐项进度通过 `on_event` 回调上报,结束返回汇总,由上层把
//! 成功路径批量同步到内存模型(见 `ScanResult::apply_deleted`)。
//!
//! - Windows 上 `trash::delete` 走 Shell COM 的 IFileOperation,单次可能耗时数十秒;
//!   trash 按线程初始化 COM(thread_local),因此多个线程可各自独立调用;
//! - 并发上限控制在 4,避免大量 Shell 操作互相争用反而变慢;
//! - 取消只作用于"尚未开始的项":单次的 shell 调用无法中断,但可跳过剩余项;
//! - `catch_unwind` 兜住 COM 初始化等 panic,保证每个工作线程都能收尾。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

/// 并发删除的工作线程上限
const MAX_DELETE_WORKERS: usize = 4;

/// 单项删除结果
pub struct DeleteOutcome {
    pub path: PathBuf,
    /// None = 已成功移到回收站
    pub error: Option<String>,
}

/// 工作线程 → 上层的事件
pub enum DeleteEvent {
    /// 开始处理某一项(状态栏据此显示当前路径)
    Started(PathBuf),
    /// 某一项处理完毕
    Item(DeleteOutcome),
}

pub struct DeleteSummary {
    /// 成功移到回收站的路径(交给 `ScanResult::apply_deleted`)
    pub succeeded: Vec<PathBuf>,
    /// 失败信息("路径 — 错误")
    pub errors: Vec<String>,
    pub ok: usize,
    pub done: usize,
    pub total: usize,
    pub cancelled: bool,
}

/// 并行执行一批删除,在调用线程汇总事件并转发(回调仍保持单线程语义)。
/// 函数返回时所有工作线程已结束,任务一定收尾。
pub fn run_delete<F: FnMut(DeleteEvent)>(
    paths: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
    mut emit: F,
) -> DeleteSummary {
    let total = paths.len();
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .min(MAX_DELETE_WORKERS)
        .min(total.max(1));

    // 轮转分片:各线程处理自己的一批;事件顺序可能交错,前端只做计数不受影响
    let mut chunks: Vec<Vec<PathBuf>> = vec![Vec::new(); workers];
    for (i, p) in paths.into_iter().enumerate() {
        chunks[i % workers].push(p);
    }

    let (tx, rx) = mpsc::channel::<DeleteEvent>();
    let mut summary = DeleteSummary {
        succeeded: Vec::new(),
        errors: Vec::new(),
        ok: 0,
        done: 0,
        total,
        cancelled: false,
    };

    std::thread::scope(|scope| {
        for chunk in chunks {
            let tx = tx.clone();
            let cancel = Arc::clone(&cancel);
            scope.spawn(move || {
                for path in chunk {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if tx.send(DeleteEvent::Started(path.clone())).is_err() {
                        return; // 汇总端已关闭
                    }
                    // trash 在 Windows 上按线程 CoInitializeEx,失败时会 panic;
                    // 转成普通错误,避免任务静默僵死。
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        trash::delete(&path)
                    }));
                    let error = match result {
                        Ok(Ok(())) => None,
                        Ok(Err(e)) => Some(e.to_string()),
                        Err(payload) => {
                            Some(format!("删除线程内部错误:{}", panic_message(&*payload)))
                        }
                    };
                    if tx
                        .send(DeleteEvent::Item(DeleteOutcome { path, error }))
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
        drop(tx);

        // 由调用线程统一消费事件,保证 on_event 不会被多个线程同时调用
        for ev in rx {
            match &ev {
                DeleteEvent::Started(_) => {}
                DeleteEvent::Item(outcome) => {
                    summary.done += 1;
                    match &outcome.error {
                        None => {
                            summary.ok += 1;
                            summary.succeeded.push(outcome.path.clone());
                        }
                        Some(err) => summary
                            .errors
                            .push(format!("{} — {err}", outcome.path.display())),
                    }
                }
            }
            emit(ev);
        }
    });

    summary.cancelled = cancel.load(Ordering::Relaxed);
    summary
}

/// 把 panic payload 转成可读文本
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}
