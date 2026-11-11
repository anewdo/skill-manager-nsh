# Icon 使用规范

## 1. 方案

nsh 不提供内置图标。项目内所有图标集中定义在 [icon.tsx](../components/icon.tsx)：每个图标是一段 lucide 的 SVG 路径数据，由 `Image` 以 `data:image/svg+xml` 的 data URI 渲染。

颜色来自 theme token：`Icon` 读取 `color` 指定的 token 当前值，把它写进 SVG 的 `stroke`（以及路径数据中标记为 `#000000` 的 `fill`），按「图标 + 颜色」缓存生成的 data URI。token 随浅色 / 深色主题变化，图标随之重绘，一个图标只需要一份路径数据。

不使用 `Image` 的 `tintColor`。nsh 0.4.0 的 Windows 宿主上，带 `tintColor` 的图片处在淡出的 `Dialog` 内时，该帧绘制失败（Direct2D `0x8899001E`，日志为 `drawing failed`），窗口停止重绘。

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
| `circleCheck` | `circle-check` | 成功提示、连通性正常 |
| `minus` | `minus` | 复选框的部分选中 |
| `package` | `package` | 技能库为空 |
| `searchX` | `search-x` | 搜索或筛选无结果 |

## 3. 使用方式

```tsx
import { Icon } from "../../components/icon";

<Icon name="trash" color="destructive" accessibilityLabel="删除" />
```

| prop | 说明 |
| --- | --- |
| `name` | `iconSources` 的键，即上表的 `name` |
| `size` | 绘制尺寸（点），默认 16 |
| `color` | theme token，默认 `muted-foreground`；可取 `foreground`、`primary`、`primary-foreground`、`destructive`、`success`、`warning`、`info` 等 |
| `class` | 额外的布局类，尺寸仍由 `size` 决定 |
| `accessibilityLabel` | 图标的无障碍语义，与所在按钮文案一致 |
| `testID` | 测试定位 |

页面里的按钮不直接拼 `Icon`：带文案的按钮用 [components/ui](../components/ui/button.tsx) 的 `Button`（`icon` 传 `IconName`），只有图标的按钮用 `IconButton`（`label` 同时是无障碍文案和悬停提示，`tone="danger"` 时图标取 `destructive`）。

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
- 图标必须是单色描边模板（`fill="none"`）；需要填充的小元素把 `fill` 写成 `#000000`，`Icon` 会把它换成图标颜色。多色图标不支持。
- SVG 不引用外部字体、图片、CSS 或脚本。
- 逻辑尺寸固定 24×24（`viewBox="0 0 24 24"`），显示尺寸由 `Icon` 的 `size` 控制，保证任意尺寸清晰。
- 图标按项目界面的动作语义收录（导航、搜索、刷新、更新、安装、预览、删除、重命名、关闭、翻页、视图切换、状态提示）；新界面需要表中没有的语义时再按第 4 节添加，不预置与项目无关的图标。

## 6. 验证

- `nsh check` 与 `nsh test App.test.tsx` 通过。
- 改动图标的绘制方式后，必须在 `--target windows` 的真实窗口里打开并关闭一个含图标的对话框，确认日志没有 `drawing failed`。无界面测试不绘制，发现不了这类问题。
