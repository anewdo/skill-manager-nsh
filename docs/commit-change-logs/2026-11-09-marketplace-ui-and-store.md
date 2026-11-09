# Marketplace 页面与前端基础设施

日期：2026-11-09

## 功能概述

- 新增 MarketPlace 页面：总榜 / 趋势 / 热门、60 条搜索、来源筛选、每页 24 条三列分页、显式预览、安装进度与结果提示。
- 新增前端 store、i18n、主题应用与结构化错误处理，统一管理设置、安装列表、活动操作和操作快照。
- GitHub 来源的 Skill 改为按 frontmatter 名称解析真实仓库目录，修复 skills.sh 条目（如 `find-skills` 位于 `skills/find-skills`）无法预览和安装的问题。
- 市场榜单与搜索返回抓取时间与 stale 标记，缓存键加版本前缀。

## 前端改动

| 文件 | 说明 |
| --- | --- |
| `store.ts` | `createAppStore()`：设置、市场安装列表、活动操作、操作快照、加载状态和错误；先订阅 `subscribeOperations` 再加载数据；处理 `operation-state`、`operation-progress`、`installations-changed`、`operations-resync`；安装列表查询带请求序号防旧响应覆盖；启动标记防止重复订阅；启动时恢复活动操作。 |
| `errors.ts` | `toAppError()` 按 `code` 白名单规范化原生错误，`describeError()` 输出本地化标签并保留原始 message，`conflictInstallationId()` 读取 `details.installationId`。 |
| `i18n.ts` | 中英文完整文案、`{param}` 替换和消息键导出，覆盖应用、通用、市场、错误码文案。 |
| `components/ui.tsx` | `ActionButton`、`Banner`、`Badge` 三个通用组件，只使用现有设计 token。 |
| `pages/marketplace/Marketplace.tsx` | 页面主体：榜单切换、350ms 防抖搜索、来源筛选、分页、卡片网格、预览对话框、安装进度与取消、stale 与错误横幅、已安装提示；榜单、搜索、预览请求均带序号防覆盖；卸载时清理防抖定时器。 |
| `pages/marketplace/model.ts` | 页面纯逻辑：每页条数、搜索条数、来源过滤、分页与范围、三列分行、已安装 lineage 集合、按 ID 查找、数量与时间格式化、去除 SKILL.md frontmatter。 |
| `pages/marketplace/model.test.ts` | 10 个前端测试覆盖上述逻辑。 |
| `App.tsx` | 创建 store 并启动，标题栏、全局设置与同步错误横幅、挂载 Marketplace 页面。 |
| `App.test.tsx` | 删除模板计数器测试。 |
| `api.ts` / `types.ts` | `MarketplaceSkill.lineage`、`MarketDataset` 类型；`marketLeaderboard` / `marketSearch` 返回 `MarketDataset`。 |

## 后端改动

| 文件 | 说明 |
| --- | --- |
| `src/core/market.rs` | `MarketDataset { skills, fetched_at, stale }`；缓存键加 `v2` 版本前缀；新鲜缓存与网络成功为 `stale: false`，仅网络故障回退缓存时标记 `stale: true` 并保留缓存写入时间；无缓存时保留原始结构化错误；重写并补充缓存版本隔离、stale 回退、失败保留错误等测试。 |
| `src/core/sources/skills_sh.rs` | `MarketplaceSkill` 新增 `lineage: SkillSource`，`id` 使用 source key；测试断言同步更新。 |
| `src/core/sources/github.rs` | 用 `git ls-tree -r -z HEAD` 列出仓库内 SKILL.md，并用单次 `git cat-file --batch` 读取 frontmatter 名称；按名称精确匹配、再按路径或末段目录名匹配，唯一命中后对该真实目录做 sparse checkout；仓库根 SKILL.md 关闭 sparse checkout；无匹配返回 `INVALID_SKILL`，多匹配返回带 `details.candidates` 的 `AMBIGUOUS_SKILL`；SKILL.md 读取上限 1 MiB、候选上限 256；新增按名称解析、仓库根、同名歧义、名称缺失四个本地 Git 集成测试。 |
| `src/core/model.rs` | `SkillSource` 增加 `Eq` 派生。 |
| `src/api/mod.rs` | 市场方法文档注释补充抓取时间与 stale 状态。 |

## 工程改动

- `.gitignore` 增加 `temp/`。
- `AGENTS.md` 调整项目定位措辞与转义。

## 验证

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`nsh check` 通过。
- `nsh test`：116 个测试全部通过（106 Rust + 10 前端）。
- Windows debug 实机验证（`nsh test --target windows App.tsx`，实时 skills.sh 数据）：
  - 首屏 600 条，每页 24 条，分页 25 页，显示“显示 1-24，共 600 条”。
  - 第 2 页显示“显示 25-48，共 569 条”。
  - 来源筛选“网站”后显示 31 条。
  - 预览 `find-skills`（`vercel-labs/skills`）返回真实 SKILL.md，对话框居中，正文不含 frontmatter。
  - 安装 `find-skills` 成功，显示下载阶段与取消按钮，完成后提示“已安装「find-skills」”，卡片出现已安装徽标并刷新安装列表。

## 遗留

- 离线 stale cache、同内容复用、同 lineage 冲突提示和 `operations-resync` 尚未在实机模拟，由 Rust 测试和前端状态逻辑覆盖。
- My Skills 与 Settings 页面未实现，语言与主题切换入口随 Settings 页面提供。
