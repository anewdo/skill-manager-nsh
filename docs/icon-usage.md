# Icon 使用规范

## 1. 结论

nsh 不提供内置图标组件或图标库，`Image` 的 `src` 接受导入的本地资源（`ImageAsset`）、`asset:` 路径或 URL。项目内的所有图标以静态 SVG 文件自备，统一放在 `assets/icons/`。

这些 SVG 属于源码资产：新增、替换图标都需要提交文件，并在使用处 `import` 后交给 `Image` 渲染。

## 2. 现有图标

| 文件 | 来源 | 用途 | 颜色 |
| --- | --- | --- | --- |
| `assets/icons/trash.svg` | lucide-static v1.70.0 `trash-2`（ISC） | 技能库所有删除入口 | `#dc2626` |

渲染入口是 [ui.tsx](file:///d:/MyCode/App/Rebuild/skills-manager-nsh/components/ui.tsx) 的 `IconButton`，列表行、卡片、详情对话框与批量删除都复用它。

## 3. 使用方式

```tsx
import trashIcon from "../../assets/icons/trash.svg";
import { Image } from "nsh";

<Image src={trashIcon} class="h-4 w-4" accessibilityLabel="删除" />
```

- 图标通过 `import` 变成 `ImageAsset`，由打包器内联进构建产物；未被任何文件引用的 SVG 不会被打包。
- 显示尺寸用 `class` 控制（`h-4 w-4`）；SVG 的 `width` / `height` 保持 16，`viewBox` 保持 `0 0 24 24`，保证缩放清晰。
- 每个图标都要提供 `accessibilityLabel`；与按钮语义重复时与按钮文案保持一致。

## 4. 硬性约束

### 4.1 颜色必须写具体值

`Image` 没有 tint 属性，nsh 不解析 `currentColor`、CSS 变量或 `class`。图标颜色只能固化在 SVG 的 `stroke` / `fill` 上。

- 危险操作（删除）：`#dc2626`，对应主题的 destructive 语义。
- 普通图标：按当前主题取具体色值（浅色主题使用深灰，深色主题使用浅灰）；需要跟随主题的图标要准备两份文件，由调用方按主题切换 `src`。

### 4.2 保持 SVG 自包含

- 不引用外部字体、外部图片、CSS 或脚本。
- 删除编辑器导出的 `class`、`style`、`data-*` 冗余属性。
- 首行保留来源与许可注释，例如 `<!-- lucide-static v1.70.0, ISC License -->`。

### 4.3 命名与放置

- 文件名使用 kebab-case，与图标含义一致，例如 `trash.svg`、`chevron-down.svg`。
- 只把真实使用的图标放进 `assets/icons/`；临时素材放在 `temp/`。

## 5. 从 lucide 下载图标

lucide 官方的 `lucide-react` / `lucide` 依赖 React，不能用于 nsh。使用它的静态 SVG 产物 `lucide-static`，按需取单个文件。

1. 在 https://lucide.dev/icons/ 搜索图标，确认名称，例如 `trash-2`。
2. 任选一种获取方式：
   - 图标详情页点击 Download SVG；
   - 直接从 unpkg 取：`https://unpkg.com/lucide-static/icons/<name>.svg`；
   - 安装 `lucide-static` 后从 `node_modules/lucide-static/icons/<name>.svg` 取。一次性使用推荐前两种。
3. 本项目所在机器访问 GitHub 或 unpkg 需要代理，下载命令：

```powershell
curl.exe -sSL --proxy http://127.0.0.1:7897 `
  -o assets/icons/<name>.svg `
  https://unpkg.com/lucide-static/icons/<name>.svg
```

4. 处理下载结果：
   - 删除 `class` 属性；
   - `width` / `height` 改为 `16`；
   - `stroke="currentColor"` 改为目标颜色；
   - 保留 `viewBox="0 0 24 24"`、`fill="none"`、`stroke-linecap` 与 `stroke-linejoin`；
   - 在首行补来源与版本注释。
5. 验证：

```powershell
nsh check                 # 资源导入与类型检查
nsh test App.test.tsx     # 打包 App 并在无头宿主渲染
nsh test --target windows App.test.tsx   # 真实 Windows 宿主渲染
```

构建日志里的 `bundled App.tsx (... B)` 体积变化可以确认新 SVG 已进入产物。

## 6. 已验证记录

2026-11-10 通过上述流程下载 lucide-static v1.70.0 的 `trash-2.svg`，处理为 `assets/icons/trash.svg`（16×16、`viewBox 0 0 24 24`、`stroke="#dc2626"`）。

验证结果：

- `nsh test App.test.tsx`：1 passed / 0 failed，bundle 108970 B；
- `nsh test --target windows App.test.tsx`：真实 Windows 宿主 1 passed / 0 failed。

## 7. 其他图标集

Tabler Icons、Heroicons、Material Symbols 等提供原始 SVG 的图标集同样可用，条件一致：静态 SVG、颜色写死、无外部依赖。选用时在文件首行注明来源与许可。
