# nsh 文档/API 查询步骤（Agent 专用）

如果遭遇GitHub访问失败请尝试利用本地代理`127.0.0.1:7897`进行访问

## 1. 首选：Context7

库 ID 已知，跳过 resolve，直接查：

```bash
npx -y ctx7@latest docs /longbridge/nsh "<完整英文问题，含组件/API 名>"
```

- 每个任务最多 3 次调用；query 写具体（如 "DataTable sorting pagination props"），不写单词。
- 备用 ID：`/websites/nsh_dev`、`/websites/deepwiki_longbridge_nsh`。
- 仅当怀疑 ID 失效时才执行 `npx -y ctx7@latest library nsh "<query>"`。

## 2. 兜底：官方文档源

1. 总索引（先查这里定位文件）：`https://nsh.dev/llms.txt`
2. 原文优先取 raw：`https://raw.githubusercontent.com/longbridge/nsh/main/website/<路径>`
3. raw 失败（DNS/网络）时改用 WebFetch 抓 `https://github.com/longbridge/nsh/blob/main/website/<路径>`，仍失败回到 Context7。

## 3. 路径规则（website/ 下）

| 查什么 | 路径 |
|---|---|
| 概念/CLI/样式/布局 | `docs/<主题>.md`（cli、styling、tailwind、layout、lists、rust、project-structure） |
| 组件 API/Props | `elements/<组件名>.md`（kebab-case，如 data-table、date-picker、nav-stack、rich-editor） |
| 运行时模块/样式参考 | `modules/<模块>.md`（nsh、nsh-base、nsh-desktop、nsh-list、tailwind-reference、style-reference） |

组件/模块完整清单：用 GitHub API 列目录：

```bash
curl -s "https://api.github.com/repos/longbridge/nsh/contents/website/elements" | grep '"name"'
```

（`modules` 同理。）

## 4. 禁止事项

- 不猜 URL：`nsh.dev/docs/elements` 等聚合页不存在（404），组件文档只在 `elements/` 单文件路径。
- 查不到就明说，不凭训练数据编造 nsh API（项目极新，训练数据不可靠）。