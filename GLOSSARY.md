# Skills Manager

Skills Manager 维护可发现的 Skill、独立的本地 Skill 集合、它们与远端来源之间的关系，以及它们在各 Agent 平台上的部署。

## Language

**Skill**:
以 `SKILL.md` 描述内容和使用方式的一组文件。
_Avoid_: 插件、扩展

**Marketplace Entry**:
市场中可浏览或搜索到的一项 Skill，代表一个来源中的 Skill，而不是已经安装的本地副本。
_Avoid_: 已安装 Skill、应用

**Installation Lineage**:
Installed Skill 与其 Skill Source 之间持续存在的关系；远端内容变化形成同一 Lineage 的新版本，而不是自动形成另一项安装。
_Avoid_: Identical Skills、显示名称

**Suffixed Name**:
同名但内容不同的 Installed Skill 所使用的本地名称，首个同名安装使用普通名称，后续依次使用 `-2`、`-3` 等数字后缀，例如 `skill-name-2`。
_Avoid_: Scoped Name、owner 简写、Installation Lineage

**Manual Rename**:
用户为一条 Installed Skill 指定新的显示名和本地根目录名，不改变其文件内容、来源或 Installation Lineage。
_Avoid_: 重命名远端 Skill、重新安装、修改 frontmatter

**Skill Source**:
Skill 的规范化远端出处；同一来源中的同一 Skill 具有稳定身份。
_Avoid_: 下载地址、安装目录

**Installed Skill**:
Central Repository 中受应用管理的一份 Skill 及其来源、版本和状态。所有 Installed Skill 属于同一个技能库，名称在库内唯一；是否具有 Skill Source 只决定它能否 Update。
_Avoid_: Market Entry、快捷方式、Marketplace Installation、Local Installation

**Local Import**:
从本地文件夹或压缩包加入技能库、不具有 Skill Source 的 Installed Skill 的加入方式；这样的 Skill 不能 Update。
_Avoid_: 本地类别、Local Installation

**Identical Skills**:
技能库内，两份 Skill 的相对文件路径集合以及每个文件的规范化内容均完全一致；UTF-8 文本的 CRLF/LF 和末尾一个换行不影响判同，二进制仍逐字节比较。仅同名、描述相似或部分文件相同都不属于 Identical Skills。不同来源的远端内容变化不改变已有 Installation Lineage。
_Avoid_: 同名 Skill、Installation Lineage、相似 Skill

**Central Repository**:
新应用独立维护的本地 Installed Skill 集合，不与原应用共同读写。
_Avoid_: Agent 目录、云端备份

**Content Baseline**:
Installed Skill 上次成功安装或更新时所对应的内容状态，用于识别此后的本地变化。
_Avoid_: 当前文件哈希、远端版本号

**Local Modification**:
Installed Skill 的当前内容相对 Content Baseline 发生的任何增加、删除或修改。
_Avoid_: 更新、未提交更改

**Update**:
将 Installed Skill 的内容推进到 Skill Source 的较新版本，不隐含覆盖 Local Modification 的权利。
_Avoid_: 重新安装、同步

**Preview**:
在不安装 Skill 的情况下查看其远端内容。
_Avoid_: 安装、更新

**Preset**:
由用户组织并命名的一组 Installed Skill。Preset 本身不部署任何东西，只在被某个 Deploy Target 启用时成为其 Desired State 的一部分。
_Avoid_: 场景、安装批次、标签

**Agent**:
一个能够从约定目录读取 Skill 的 Agent Harness 平台，例如 Claude Code、Codex。
_Avoid_: 工具、Tool、平台适配器

**Deploy Target**:
一个真实存在的 Skill 根目录，Agent 从中读取 Skill。多个 Agent 约定同一目录时共用一个 Deploy Target。
_Avoid_: Agent、同步目标

**Deployment**:
应用在某个 Deploy Target 中为一份 Installed Skill 放置的链接或副本的记录。它记录的是应用过去做过什么，不证明磁盘上现在的内容归应用所有。
_Avoid_: 同步、安装

**Desired State**:
用户希望某个 Deploy Target 拥有的 Installed Skill 集合，等于其启用的 Preset 的成员与单独启用的 Skill 的并集。
_Avoid_: 已部署列表、当前 Preset

**Reconcile**:
比较 Deploy Target 的 Desired State 与其 Deployment 及磁盘现状，并新增、移除或修复 Deployment 使二者一致。Reconcile 不触碰不属于应用的文件。
_Avoid_: 同步、应用 Preset
