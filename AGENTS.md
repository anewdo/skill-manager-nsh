# Skills Manager NSH 项目规则

## 项目定位

使用 nsh 构建桌面端App--Skills Manager。核心功能包括 skills.sh 市场、显式预览、安装、技能库、手动更新和设置

## 技术栈

- nsh / nsh-core `0.4.0`，TSX 原生 UI，不使用 React、Web DOM 或 Tauri API。
- Rust 2024；`src/api/` 负责 nsh 接口边界，`src/core/` 负责可测试的领域与基础设施逻辑。
- SQLite（rusqlite）、serde / serde\_json、reqwest、Git CLI、SHA-256、ZIP、tokio broadcast。
- TypeScript 通过 `nativeModule("skills-manager-nsh/native")` 调用 Rust。

## 项目结构

```text
App.tsx / App.test.tsx     # nsh UI 入口与测试
store.ts                   # 应用状态与操作跟踪
lib/                       # api.ts（原生 API 封装）、types.ts（前端 DTO）、errors.ts（错误映射）
i18n/                      # zh_CN.ts / en.ts 文案表与 index.ts（translate、MessageKey）
components/                # 通用 UI 组件
pages/                     # 市场、技能库、设置页面
nsh.json                   # mixed 工程配置，rust = "."
Cargo.toml                 # Rust crate 配置
src/lib.rs                 # native_module 注册；无 src/main.rs
src/api/mod.rs             # AppNative、JsError、blocking、Callback
src/core/                  # 模型、Store、路径、队列、安装等核心逻辑
src/core/sources/          # skills.sh、GitHub、well-known、预览与下载
.nsh/types/                # nsh 自动生成的声明
docs/plans/                # 已确认的产品与实施计划
docs/progress/             # 分模块实现状态与验证记录
docs/todo/                 # 后续工作安排
```

## nsh 开发注意事项

- 以本机安装的 nsh `0.4.0` 为准
- 查询 nsh API 或行为时，先读 `docs/NSH-DOCS-LOOKUP.md`；优先 Context7 `/longbridge/nsh`，每项最多 4 次查询，不臆造 API 或文档地址
- 原生服务通过 `#[module]` 注册；Rust snake\_case 方法会生成 TypeScript camelCase 名称
- 修改原生接口后同步更新 `lib/api.ts` 和 `lib/types.ts`，运行 `nsh check` 并核对生成声明；`.nsh/types/` 不手工编辑
- async 原生方法映射为 Promise。只读长任务使用 `nsh_core::prelude::blocking`，写操作统一进入 FIFO `OperationQueue`。
- Git、HTTP、哈希、ZIP 和批量文件 IO 不得占用 UI 引擎线程；操作事件使用 `Callback<JsonValue>.post(...)`。
- 原生错误保留 `code`、`message`、`retryable` 和可选 `details`，前端通过字段处理，不解析错误字符串。
- UI 使用 nsh 组件和 `signal` 响应式模型，不套用 React hooks 或浏览器全局对象。
- 涉及 UI 的改动必须在 nsh debug 中实际操作黄金路径和边界情况；类型检查和模板测试不能代替功能验收。

## 核心业务约束

- 只使用独立的 `~/.skills-manager-nsh/`，不得读取或修改原项目的 `~/.skills-manager` 数据
- Preview 只在用户显式调用时联网；启动、打开详情和列表加载不得隐式触发预览下载
- 拒绝路径逃逸、链接、特殊文件、危险 ZIP 条目、重复路径和超限归档
- 文本内容身份统一 CRLF/LF 并忽略末尾一个换行；二进制保持字节精确，哈希候选必须再经完整文件树判同
- 同 source lineage 走更新；同内容不同来源复用安装；同名不同内容使用数字后缀（`skill-2`、`skill-3`）
- 不得静默覆盖本地修改。目录 rename 后进入不可取消提交边界，数据库失败必须恢复文件系统状态
- 仅在真实提交后发送安装变更事件；UI 和设置页只展示已经实现的能力

## 验证与文档

完成 Rust 或接口改动后运行：

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
nsh check
nsh test
```

> 测试尽量仅运行本次改动相关部分而非全量运行

评估已实现与未实现边界时读 `docs/progress/` 最新文档；安排后续任务时读 `docs/todo/` 最新文档；核对产品范围时读 `docs/plans/2026-11-09-marketplace-debug.md`。功能完成或验证结果变化时同步更新进度与待办文档。
