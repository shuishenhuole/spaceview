# 更新日志

本项目版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/) 规范。

## [未发布]

### 变更

- 项目重命名:Cargo 包名与库名统一为 `spaceview`,可执行文件为 `spaceview.exe`。
- 工具显示名改为 **SpaceView**,窗口标题、界面标题与 Windows 安装包(产品名、安装目录、快捷方式)同步更新。
- 顶层应用结构体 `AnalyseApp` 更名为 `SpaceViewApp`。

## [1.0.1] - 2026-10-28

### 新增

- 增加 GitHub Actions 持续集成流水线:在 Windows / macOS / Linux 三平台构建 release 并运行测试;推送到 master 后自动创建 Release 并上传各平台构建产物。

### 重构

- 新增 `src/lib.rs` 作为库入口,导出 `app`、`engine`、`i18n`、`ui` 模块;`src/main.rs` 简化为只负责窗口、图标与字体初始化的瘦入口。
- `AnalyseApp` 新增 `restore_prefs()` 与 `set_target_folder()`,用于从 eframe Storage 恢复主题 / 语言 / 动效偏好,以及设置命令行传入的初始扫描目录。

### 修复

- 修复 `cargo test` 集成测试中由源码重复编译产生的大量 `dead_code` 警告:测试不再通过 `#[path]` 复制 `i18n`、`tree`、`scanner` 源码,改为直接链接 `analyse` 库。
- 测试目标不再重复编译引擎代码,缩短测试构建时间。

## [1.0.0] - 2026-10-24

- 首次发布
