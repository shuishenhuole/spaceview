# SpaceView（Tauri 2 重构版）

一个磁盘空间分析桌面工具，定位类似 SpaceSniffer / WizTree：并行扫描目录树、实时展示扫描进度，并以大文件列表、类型统计和 Treemap 矩形树三种视角帮你找出占空间的「元凶」。

> 旧版实现（Rust + eframe / egui 0.36）完整保留在 [`old/`](./old/) 目录，供对照参考。本目录是它的 **Tauri 2 重构**：egui 界面层换成 Web 前端，扫描/删除引擎与算法逻辑保留在 Rust 侧。

## 从 egui 迁移到了什么

| 层 | 旧版（egui） | 重构版（Tauri 2） |
| --- | --- | --- |
| 界面 | eframe / egui 0.36 即时模式 | Vue 3 + TypeScript + [shadcn-vue](https://www.shadcn-vue.com/) |
| 样式 | egui 内置 Visuals + DWM API | Tailwind CSS v4（工具类） + Less（主题变量 / 动画 / 全局样式） |
| 扫描引擎 | `engine/scanner.rs`（jwalk） | 保留，去掉 egui 依赖，改由 Tauri command 驱动 |
| 目录树 / 删除 | `engine/tree.rs`、`deleter.rs` | 保留，结果模型仍在 Rust 侧同步 |
| Treemap 布局 | `engine/treemap.rs`（egui `Rect`） | 移植为纯 TS 算法 `src/lib/treemap.ts`，前端渲染 |
| 线程通信 | `std::sync::mpsc` | Tauri 事件 `scan://*` / `delete://*` |
| 文件夹选择 / 打开路径 / 剪贴板 | rfd / `std::process::Command` | `tauri-plugin-dialog` / `-opener` / `-clipboard-manager` |
| 偏好持久化 | eframe Storage | `localStorage` |

分层原则与旧版一致：**`src-tauri/src/engine/` 是纯数据与算法，不含任何 UI / Tauri 代码；前端组件只渲染并上报动作，行为统一在 `src/App.vue` 编排。**

## 目录结构

```
src/                              # Web 前端
├── App.vue                       # 状态机编排:扫描阶段、过滤/排序/下钻/勾选/偏好
├── api.ts                        # Tauri commands 与事件订阅封装
├── types.ts                      # 与 Rust DTO 一一对应的类型
├── i18n.ts                       # 中 / 英双语字典
├── style.css                     # Tailwind v4 入口 + @theme inline 映射
├── styles/
│   ├── theme.less                # 浅色 / 深色主题 tokens(Less 变量)
│   └── app.less                  # 全局布局、质感、滚动条、Treemap 动画
├── lib/
│   ├── format.ts                 # humanSize / 路径 / 截断等展示工具
│   └── treemap.ts                # Squarify 布局算法 + 块收集 + 配色
└── components/
    ├── ui/                       # shadcn-vue 生成的组件(Button/Select/Tabs/Dialog…)
    ├── Toolbar.vue               # 盘符选择、开始/取消/暂停、主题/动效/语言
    ├── FilterBar.vue             # 名称搜索 + 大小阈值
    ├── DirectoryTree.vue         # 左侧目录树(递归 / 过滤态扁平列表)
    ├── TopFiles.vue              # 大文件 TopN(排序、多选、键盘导航)
    ├── ExtensionStats.vue        # 按扩展名占用统计
    ├── TreemapView.vue           # 下钻式矩形树(悬停详情、块形变过渡)
    ├── StatusBar.vue             # 进度 / 速度 / 完成摘要 / 删除进度
    ├── RowContextMenu.vue        # 复制路径 / 打开所在目录 / 删除到回收站
    ├── ConfirmDelete.vue         # 删除确认对话框
    └── Toast.vue                 # 操作反馈气泡

src-tauri/                        # Rust 后端
├── src/
│   ├── lib.rs                    # Tauri Builder、插件、commands 注册
│   ├── commands.rs               # 前端唯一入口 + 后台线程 + 事件推送
│   ├── state.rs                  # 共享状态(取消/暂停标志、扫描结果、epoch)
│   └── engine/
│       ├── mod.rs
│       ├── scanner.rs            # jwalk 并行遍历、每秒部分快照、TopN/扩展名
│       ├── tree.rs               # 目录 Arena、聚合、删除后模型同步
│       ├── deleter.rs            # trash 后台删除
│       └── dto.rs                # 内存结构 → 前端 DTO(camelCase)
├── tests/engine.rs               # 引擎单元测试(5 项)
├── capabilities/default.json     # 插件权限
└── tauri.conf.json
```

## 前后端通信协议

**Commands（前端 → Rust）**

| command | 说明 |
| --- | --- |
| `list_drives` | 枚举盘符 / 挂载点 |
| `start_scan(root)` | 启动后台并行扫描 |
| `cancel_scan` | 请求取消当前扫描 |
| `set_paused(bool)` | 暂停 / 继续（耗时与速度在暂停时冻结） |
| `delete_paths(paths)` | 勾选路径批量移到回收站 |
| `cancel_delete` | 跳过尚未开始的删除项 |

**Events（Rust → 前端）**

| event | payload |
| --- | --- |
| `scan://tick` | 目录数 / 文件数 / 字节数 / 当前路径 |
| `scan://snapshot` | 扫描中的部分结果（每秒一次，边扫边看） |
| `scan://done` | 扫描完成结果 |
| `scan://updated` | 删除后由 Rust 同步好的新结果模型 |
| `scan://cancelled` / `scan://failed` | 空 / 错误信息 |
| `delete://started` / `delete://item` / `delete://done` | 删除逐项进度与汇总 |

## 功能特性

- **并行扫描**：jwalk 多线程目录遍历；扫描期间每秒推送快照，三个视图实时展示已扫描内容
- **暂停 / 继续 / 取消**：暂停时耗时、速度等统计数字冻结不动
- **大文件 TopN**：按大小 / 名称排序，支持 ↑↓ 光标、空格勾选、Ctrl/Cmd+A 全选、Shift 范围选择
- **按类型统计**：按扩展名聚合大小与数量
- **Treemap 矩形树**：下钻式空间占比图，块形变 / 淡入过渡，悬停显示路径、大小、占比
- **联动导航**：点击目录树定位 Treemap；双击文件打开所在目录；Backspace / 面包屑返回上一级
- **过滤**：名称搜索 + 大小阈值，作用于文件列表与目录树
- **删除到回收站**：勾选后经确认移入回收站（trash），安全可恢复；失败项自动回到勾选集
- **深色 / 浅色主题**、**中 / 英双语**、**减少动画**：一键切换并持久化
- **受限目录容错**：无权限目录自动跳过并汇总列出，不中断扫描

## 开发

环境：Node.js 20+、Rust 1.85+（edition 2024）、Tauri 2 的系统依赖（Windows 需 WebView2；Linux 需 `libwebkit2gtk-4.1-dev` 等）。

```bash
npm install
npm run tauri dev          # 启动 Vite + Tauri 调试窗口
```

单独跑前端（浏览器里没有 Tauri API，仅用于样式开发）：

```bash
npm run dev
```

## 构建与校验

```bash
npm run build              # vue-tsc --noEmit && vite build
npm run tauri build        # 打包桌面应用

cd src-tauri && cargo test # 引擎单元测试(聚合 / TopN / 扩展名 / 删除模型同步)
```

## 许可

[MIT](./old/LICENSE) © 2026 shuishenhuole
