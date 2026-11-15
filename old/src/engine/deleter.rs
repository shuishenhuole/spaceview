//! 后台删除引擎:把"移到回收站"这一可能很慢的 I/O 放进工作线程,
//! UI 线程只消费事件、累计进度,并在任务结束后一次性同步内存模型。
//!
//! 设计要点:
//! - Windows 上 `trash::delete` 走 Shell COM 的 `IFileOperation::PerformOperations`,
//!   大目录/网络盘可能耗时数十秒;该 crate 按线程初始化 COM(thread_local),
//!   因此工作线程可独立调用,不依赖 UI 线程的消息泵。
//! - 内存模型(目录树/TopN/统计)由 UI 线程持有,不跨线程共享,避免渲染被锁阻塞。
//! - 取消只作用于"尚未开始的项":单次的 shell 调用无法中断,但可跳过剩余项。
//! - 用 `catch_unwind` 兜住 COM 初始化等 panic,保证通道一定以 Done 收尾或断开,
//!   不让 UI 停在一个永远等不到的状态。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// 单项删除结果
pub struct DeleteOutcome {
    pub path: PathBuf,
    /// None = 已成功移到回收站
    pub error: Option<String>,
}

/// 工作线程 → UI 线程的事件
pub enum DeleteEvent {
    /// 开始处理某一项(状态栏据此显示当前路径)
    Started(PathBuf),
    /// 某一项处理完毕
    Item(DeleteOutcome),
    /// 全部处理完毕(含被取消后提前退出)
    Done,
}

/// 一次后台删除任务在 UI 侧持有的状态
pub struct DeleteJob {
    pub rx: Receiver<DeleteEvent>,
    pub cancel: Arc<AtomicBool>,
    /// 发起删除时的扫描结果版本号,用于避免把结果同步到新一轮扫描上
    pub epoch: u64,
    /// 本批待删路径(顺序与线程处理顺序一致)
    pub paths: Vec<PathBuf>,
    pub total: usize,
    /// 已处理项数
    pub done: usize,
    /// 成功移到回收站的项数
    pub ok: usize,
    /// 成功路径:任务结束时一次性交给 `ScanResult::apply_deleted`
    pub succeeded: Vec<PathBuf>,
    /// 失败信息("路径 — 错误")
    pub errors: Vec<String>,
    /// 当前正在处理的路径
    pub current: Option<PathBuf>,
}

impl DeleteJob {
    /// 是否已请求取消剩余项
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// 启动后台删除线程。返回 UI 侧的任务句柄;函数本身立即返回,不阻塞重绘。
pub fn spawn_delete(paths: Vec<PathBuf>, cancel: Arc<AtomicBool>, epoch: u64) -> DeleteJob {
    let total = paths.len();
    let (tx, rx) = mpsc::channel();
    let work = paths.clone();
    let worker_cancel = Arc::clone(&cancel);
    thread::spawn(move || {
        for path in work {
            if worker_cancel.load(Ordering::Relaxed) {
                break;
            }
            if tx.send(DeleteEvent::Started(path.clone())).is_err() {
                return; // UI 侧已丢弃该任务,无需继续
            }
            // trash 在 Windows 上按线程 CoInitializeEx,失败时会 panic;
            // 转成普通错误,避免任务静默僵死。
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trash::delete(&path)));
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(e.to_string()),
                Err(payload) => Some(format!("删除线程内部错误:{}", panic_message(&*payload))),
            };
            if tx
                .send(DeleteEvent::Item(DeleteOutcome { path, error }))
                .is_err()
            {
                return;
            }
        }
        let _ = tx.send(DeleteEvent::Done);
    });
    DeleteJob {
        rx,
        cancel,
        epoch,
        paths,
        total,
        done: 0,
        ok: 0,
        succeeded: Vec::new(),
        errors: Vec::new(),
        current: None,
    }
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
