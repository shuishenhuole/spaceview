//! 后台删除引擎:把"移到回收站"这一可能很慢的 I/O 放进工作线程。
//!
//! 与 UI 解耦:逐项进度通过 `on_event` 回调上报,结束返回汇总,由上层把
//! 成功路径批量同步到内存模型(见 `ScanResult::apply_deleted`)。
//!
//! - Windows 上 `trash::delete` 走 Shell COM 的 IFileOperation,可能耗时数十秒;
//! - 取消只作用于"尚未开始的项":单次的 shell 调用无法中断,但可跳过剩余项;
//! - 用 `catch_unwind` 兜住 COM 初始化等 panic,保证任务一定收尾。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// 同步执行一批删除(应在工作线程中调用),函数返回时任务已全部收尾。
pub fn run_delete<F: FnMut(DeleteEvent)>(
    paths: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
    mut emit: F,
) -> DeleteSummary {
    let total = paths.len();
    let mut summary = DeleteSummary {
        succeeded: Vec::new(),
        errors: Vec::new(),
        ok: 0,
        done: 0,
        total,
        cancelled: false,
    };

    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        emit(DeleteEvent::Started(path.clone()));
        // trash 在 Windows 上按线程 CoInitializeEx,失败时会 panic;
        // 转成普通错误,避免任务静默僵死。
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trash::delete(&path)));
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(payload) => Some(format!("删除线程内部错误:{}", panic_message(&*payload))),
        };
        summary.done += 1;
        match &error {
            None => {
                summary.ok += 1;
                summary.succeeded.push(path.clone());
            }
            Some(err) => summary.errors.push(format!("{} — {err}", path.display())),
        }
        emit(DeleteEvent::Item(DeleteOutcome { path, error }));
    }

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
