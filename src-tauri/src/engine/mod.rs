//! 引擎层:目录树数据结构、后台扫描器与后台删除器。
//! 只依赖标准库与 serde,不含任何 UI / Tauri 代码,可独立做单元测试。

pub mod deleter;
pub mod dto;
pub mod junk;
pub mod scanner;
pub mod tree;
/// 解析注册表 UninstallString(跨平台纯逻辑,便于单元测试)
pub mod uninstall;
/// Windows 专用:被占用文件的解锁重试(仅 Windows 平台编译)
#[cfg(windows)]
pub mod win_lock;
