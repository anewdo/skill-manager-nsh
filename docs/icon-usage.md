# Icon 使用规范

## 1. 方案

nsh 不提供内置图标。项目内所有图标集中定义在 [icon.tsx](file:///d:/MyCode/App/Rebuild/skills-manager-nsh/components/icon.tsx)：每个图标是一段 lucide 的 SVG 路径数据，模块加载时拼成 `data:image/svg+xml` 的 data URI，由 `Image` 渲染。

颜色不写死在 SVG 里。`Icon` 把 `color` 指定的 theme token 通过 `Image` 的 `tintColor` 应用上去，图标因此跟随浅色 / 深色主题切换，一个图标只需要一份。

图标来源只允许现成图标集（本项目使用 lucide-static v1.70.0，ISC License）；禁止手绘 SVG。

## 2. 现有图标

| `name` | lucide | 用途 |
| --- | --- | --- |
| `store` | `store` | 市场导航 |
| `library` | `library` | 技能库导航 |
| `settings` | `settings` | 设置导航 |
| `palette` | `palette` | 设置-外观标题 |
| `globe` | `globe` | 设置-代理标题 |
| `folder` | `folder` | 设置-数据目录标题 |
| `search` | `search` | 搜索 |
| `refresh` | `refresh-cw` | 刷新列表、检查更新 |
| `update` | `circle-arrow-up` | 应用更新 |
| `download` | `download` | 安装技能 |
| `preview` | `eye` | 预览技能 |
| `trash` | `trash-2` | 删除 |
| `rename` | `pencil` | 重命名 |
| `close` | `x` | 关闭、清空输入 |
| `chevronLeft` / `chevronRight` | `chevron-left` / `chevron-right` | 翻页 |
| `list` | `list` | 列表视图 |
| `grid` | `layout-grid` | 卡片视图 |
| `check` | `check` | 选中、成功 |
| `warning` | `triangle-alert` | 警告 |
| `error` | `circle-alert` | 错误 |
| `info` | `info` | 提示 |

## 3. 使用方式

```tsx
import { Icon } from "../../components/icon";

<Icon name="trash" color="destructive" accessibilityLabel="删除" />
```

| prop | 说明 |
| --- | --- |
| `name` | `iconSources` 的键，即上表的 `name` |
| `size` | 绘制尺寸（点），默认 16 |
| `color` | theme token，默认 `muted-foreground`；可取 `foreground`、`primary-foreground`、`destructive` 等 |
| `class` | 额外的布局类，尺寸仍由 `size` 决定 |
| `accessibilityLabel` | 图标的无障碍语义，与所在按钮文案一致 |
| `testID` | 测试定位 |

需要“图标 + 文案”的按钮使用 [ui.tsx](file:///d:/MyCode/App/Rebuild/skills-manager-nsh/components/ui.tsx) 的 `IconButton`，其 `icon` 直接传 `IconName`，`tone="danger"` 时图标取 `destructive`。

## 4. 新增图标

1. 在 https://lucide.dev/icons/ 查图标名称，例如 `trash-2`。
2. 取该图标的 SVG 源码：

```powershell
curl.exe -sSL --proxy http://127.0.0.1:7897 `
  -o temp/<name>.svg `
  https://unpkg.com/lucide-static/icons/<name>.svg
```

3. 把 `<svg>` 内部的元素（`<path>`、`<circle>`、`<rect>`、`<line>`）原样复制进 `iconSources`，删除 `class` 等属性；`xmlns`、`width`、`height`、`viewBox`、`fill`、`stroke` 及描边属性由模块内的模板统一提供。
4. 键名用语义名（kebab-case 图标名转为 camelCase，如 `chevron-left` → `chevronLeft`），并补充上表。
5. 执行 `nsh check` 与 `nsh test App.test.tsx`。

## 5. 硬性约束

- 图标内容只能取自图标集，不手绘、不修改路径数据。
- 图标必须是单色描边模板（`fill="none"`）：`tintColor` 只重着色不透明像素，多色图标会被涂成同一个颜色。
- SVG 不引用外部字体、图片、CSS 或脚本。
- 逻辑尺寸固定 24×24（`viewBox="0 0 24 24"`），显示尺寸由 `Icon` 的 `size` 控制，保证任意尺寸清晰。
- 图标按项目界面的动作语义收录（导航、搜索、刷新、更新、安装、预览、删除、重命名、关闭、翻页、视图切换、状态提示）；新界面需要表中没有的语义时再按第 4 节添加，不预置与项目无关的图标。

## 6. 验证

- `nsh check`：通过。
- `nsh test App.test.tsx` 与 `nsh test --target windows App.test.tsx`：2 passed / 0 failed。
- Windows 宿主渲染截图确认整套图标清晰，`destructive` 等 theme token 的着色生效。
