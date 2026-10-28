//! 磁盘空间分析:引擎层(扫描、目录树、Treemap 布局)与 egui 界面层的库入口。
//! main.rs 只保留窗口启动等瘦入口,集成测试直接链接本库,
//! 避免用 `#[path]` 重复编译源码造成大量与主程序不一致的 dead_code 警告。

pub mod app;
pub mod engine;
pub mod i18n;
pub mod ui;
