//! 应用共享状态:由 Tauri `manage()` 注入,commands 与后台线程共享同一份 Arc。

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64};

use crate::engine::scanner::ScanResult;

pub struct Inner {
    /// 扫描会话版本号:每次开始扫描自增,用于丢弃过期线程的清理动作
    pub epoch: AtomicU64,
    /// 当前扫描的取消标志(每轮替换)
    pub cancel: Mutex<Arc<AtomicBool>>,
    /// 当前扫描的暂停标志
    pub paused: Mutex<Arc<AtomicBool>>,
    /// 当前删除任务的取消标志
    pub delete_cancel: Mutex<Arc<AtomicBool>>,
    /// 最近一次完成的扫描结果,删除结束后在此原地同步
    pub result: Mutex<Option<ScanResult>>,
    pub scanning: AtomicBool,
    pub deleting: AtomicBool,
    /// 残留扫描状态(与主扫描互不干扰,但不能同时跑两次)
    pub junk_scanning: AtomicBool,
    pub junk_cancel: Mutex<Arc<AtomicBool>>,
}

pub struct ScanState {
    pub inner: Arc<Inner>,
}

impl Default for ScanState {
    fn default() -> Self {
        ScanState {
            inner: Arc::new(Inner {
                epoch: AtomicU64::new(0),
                cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
                paused: Mutex::new(Arc::new(AtomicBool::new(false))),
                delete_cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
                result: Mutex::new(None),
                scanning: AtomicBool::new(false),
                deleting: AtomicBool::new(false),
                junk_scanning: AtomicBool::new(false),
                junk_cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            }),
        }
    }
}
