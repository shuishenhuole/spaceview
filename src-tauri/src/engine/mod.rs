//! 引擎层:目录树数据结构、后台扫描器与后台删除器。
//! 只依赖标准库与 serde,不含任何 UI / Tauri 代码,可独立做单元测试。

pub mod deleter;
pub mod dto;
pub mod scanner;
pub mod tree;
