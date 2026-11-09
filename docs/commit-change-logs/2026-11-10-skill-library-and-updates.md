# 技能库页面、手动更新与图标资源

日期：2026-11-10

## 功能概述

- 新增技能库（Skill Library）页面：搜索、状态筛选、列表与卡片双视图、详情、重命名、多选批量删除、手动检查更新和应用更新。
- 删除语义改为确认后物理删除：移除 trash 模型、数据库表、原生 API 与恢复/永久清理入口，远端托管的 Skill 删除后需要时从市场重新下载。
- 新增手动检查更新与应用更新链路，包含本地修改冲突策略、提交前二次校验和文件系统回滚。
- 市场重名 Skill 的命名改为在名称后追加递增数字后缀（`skill-2`、`skill-3`），移除 scoped 路径方案。
- 应用外壳改为左侧菜单栏 + 右侧内容栏。
- 新增 `IconButton` 与 SVG 图标资源，所有删除入口统一使用红色垃圾桶图标；补充图标使用与获取文档。

## 后端改动

| 文件 | 说明 |
| --- | --- |
| `src/core/migrations.rs` | schema 升级到 v4，删除 `trash` 表；migration 测试同步改为断言 `installations`、`settings` 两张表，并验证 v2 升级会同时移除市场缓存表与 trash 表。 |
| `src/core/model.rs` | 移除 `TrashEntry`、`RestoredItem`、`RestoreReport`、`PurgeReport`、`OperationKind::Restore` 与 `OperationKind::PurgeTrash`；`DeletedItem` 去掉 `trashed`；新增 `UpdateConflictPolicy`（`reject` / `overwrite`）、`UpdateCheckItem`、`CheckUpdatesReport`。 |
| `src/core/naming.rs` | `allocate_marketplace_name(skill_name, used_root_keys)` 在名称占用时依次尝试 `-2`、`-3`，填补第一个空位；移除 repository/scoped 命名；测试覆盖递增后缀与空位填充。 |
| `src/core/installations.rs` | 实现本地详情、重命名、物理删除、`check_updates` 与 `apply_update`。删除先把目录移入 `staging/delete-<uuid>`，删除数据库记录成功后清理，失败时移回原路径。更新在下载前与提交前两次校验当前哈希，本地修改默认拒绝；提交阶段把旧目录备份到 `staging/update-<uuid>`，数据库失败时恢复，只有真实提交才发送安装变更事件。重命名保留父路径，仅大小写重命名通过临时中间路径完成。 |
| `src/core/store.rs` | 移除 trash 记录的增删查；新增更新检查结果、远端 revision、baseline/current hash 与本地状态写回。 |
| `src/core/central_repo.rs` | 移除 trash 目录；保留独立的数据库、Skills、缓存、staging 与 logs 目录边界。 |
| `src/core/installations/tests/delete.rs` | 新增：物理删除即时生效、本地修改确认、批量部分失败、数据库删除失败回滚、文件缺失、取消。 |
| `src/core/installations/tests/update.rs` | 新增：批量检查结果、检查失败保留 revision、检查取消、远端变化替换、无变化仅推进 revision、本地修改拒绝与覆盖、提交前二次修改、数据库失败回滚、下载失败保留安装、文件缺失恢复、取消。 |
| `src/core/installations/tests/trash.rs` | 删除，trash 相关测试不再存在。 |
| `src/core/installations/tests/install.rs` | 同名不同内容改为断言数字后缀 `demo-2`，并验证文件落盘。 |
| `src/core/installations/tests/rename.rs` | 嵌套路径重命名测试改用普通名称，验证只替换最后一段路径。 |
| `src/api/mod.rs` | `listTrash` / `restoreTrash` / `purgeTrash` 替换为 `checkUpdates(ids)` 与 `applyUpdate(id, conflictPolicy)`；两者进入 FIFO，读取当前代理设置后执行。 |

## 前端改动

| 文件 | 说明 |
| --- | --- |
| `api.ts` / `types.ts` | 移除 trash API 与类型；新增 `checkUpdates`、`applyUpdate`、`UpdateConflictPolicy`、`UpdateCheckItem`、`CheckUpdatesReport`。 |
| `App.tsx` | 左侧 208px 菜单栏承载应用标题与市场 / 技能库导航，右侧内容栏承载全局错误横幅与当前页面。 |
| `i18n.ts` | 新增 `nav.library`（技能库 / Skill Library）与完整的 `myskills.*` 文案；市场冲突提示改为“更新请前往技能库”。 |
| `components/ui.tsx` | 新增 `IconButton`（含 `default` / `danger` 语气、可选数量角标）与 `ButtonVariant` 的 `danger`；`Badge` 增加语气参数。 |
| `assets/icons/trash.svg` | 自备 SVG 图标，lucide-static v1.70.0 `trash-2`，16×16、`viewBox 0 0 24 24`、红色描边 `#dc2626`。 |
| `pages/myskills/model.ts` | 页面纯逻辑：状态筛选与搜索组合、筛选计数、选择与全选状态、分页、卡片三列分块、来源标签、字节格式、进度百分比。 |
| `pages/myskills/model.test.ts` | 12 个前端测试覆盖上述逻辑与列表/卡片视图枚举。 |
| `pages/myskills/labels.ts` | 集中文案键、状态语气、复选框与标签页样式常量。 |
| `pages/myskills/MySkills.tsx` | 页面状态与业务流程：搜索、筛选、分页、多选、详情、重命名、删除、检查更新、应用更新；按 operation ID 跟踪操作，处理 `LOCAL_MODIFIED`、取消、部分失败与通知。 |
| `pages/myskills/components/*` | 展示层拆分：`SkillToolbar`、`SkillSelectionHeader`、`SkillPager`、`SkillEmptyState`、`SkillListItem`、`SkillCard`、`SkillActions`、`SelectionCheckbox`、`SkillDetailDialog`、`SkillRenameDialog`、`SkillDeleteDialog`、`SkillOverwriteDialog`、`SkillProgressBar`。所有删除入口复用红色 `IconButton`。 |
| `App.test.tsx` | 桌面视口冒烟测试：从左侧菜单进入技能库，列表与卡片视图可切换。 |

## 文档与工程

| 文件 | 说明 |
| --- | --- |
| `AGENTS.md` | 项目定位改为技能库；命名约束改为同名不同内容使用数字后缀。 |
| `GLOSSARY.md` | 把 `Scoped Name` 替换为 `Suffixed Name`。 |
| `docs/icon-usage.md` | 新增图标使用规范：nsh 无内置图标库、SVG 需自备、颜色必须写死、lucide-static 下载与处理步骤、验证命令。 |
| `docs/commit-change-logs/2026-11-10-skill-library-and-updates.md` | 本次变更说明。 |

## 验证

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 通过。
- `cargo test`：118 个 Rust 测试通过。
- `nsh check` 通过。
- `nsh test`：140 个测试通过（118 个 Rust + 22 个前端，含 12 个技能库纯逻辑测试与 1 个桌面视口冒烟测试）。
- `nsh test --target windows App.test.tsx`：真实 Windows 宿主 1 passed。
- 图标资源按 `docs/icon-usage.md` 流程实际下载、处理后打包验证通过。

## 遗留

- 当前本地库没有安装记录，技能库的卡片布局、红色删除图标观感，以及删除、更新的黄金路径仍需在有安装数据的 Windows debug 中实机验收。
- 网络中断提示、同内容复用、同 lineage 冲突提示和 `operations-resync` 仍需实机验收。
- Settings 页面未实现。
