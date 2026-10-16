//! 轻量国际化:全局语言开关 + 双语选取助手。
//! 语言状态存于原子变量,UI 各处经 [`b`] 即时取词,避免层层传参;
//! 偏好持久化由 app.rs 的 save() 写入 eframe Storage。

use std::sync::atomic::{AtomicBool, Ordering};

/// 当前是否英文界面(默认中文)
static EN: AtomicBool = AtomicBool::new(false);

pub fn set_en(en: bool) {
    EN.store(en, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// 按当前语言在中文/英文文案间选择
pub fn b<'a>(zh: &'a str, en: &'a str) -> &'a str {
    if is_en() {
        en
    } else {
        zh
    }
}

/// 双语 format!:两种语言的格式串共享同一组参数,按当前语言取用
#[macro_export]
macro_rules! f {
    ($zh:expr, $en:expr $(, $($arg:tt)*)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $($arg)*)?)
        } else {
            format!($zh $(, $($arg)*)?)
        }
    };
}
