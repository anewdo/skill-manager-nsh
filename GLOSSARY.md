# Skills Manager

Skills Manager 维护可发现的 Skill、独立的本地 Skill 集合，以及它们与远端来源之间的关系。

## Language

**Skill**:
以 `SKILL.md` 描述内容和使用方式的一组文件。
_Avoid_: 插件、扩展

**Marketplace Entry**:
市场中可浏览或搜索到的一项 Skill，代表一个来源中的 Skill，而不是已经安装的本地副本。
_Avoid_: 已安装 Skill、应用

**Installation Lineage**:
Marketplace Installation 与其 Skill Source 之间持续存在的关系；远端内容变化形成同一 Lineage 的新版本，而不是自动形成另一项安装。
_Avoid_: Identical Skills、显示名称

**Suffixed Name**:
同名但内容不同的 Marketplace Installation 所使用的本地名称，首个同名安装使用普通名称，后续依次使用 `-2`、`-3` 等数字后缀，例如 `skill-name-2`。
_Avoid_: Scoped Name、owner 简写、Installation Lineage

**Manual Rename**:
用户为一条 Installed Skill 指定新的显示名和本地根目录名，不改变其文件内容、来源或 Installation Lineage。
_Avoid_: 重命名远端 Skill、重新安装、修改 frontmatter

**Skill Source**:
Skill 的规范化远端出处；同一来源中的同一 Skill 具有稳定身份。
_Avoid_: 下载地址、安装目录

**Installed Skill**:
本地集合中受应用管理的一份 Skill 及其管理类别、来源、版本和状态。
_Avoid_: Market Entry、快捷方式

**Marketplace Installation**:
从 Marketplace Entry 安装并可追踪其远端来源的 Installed Skill。
_Avoid_: 本地导入、外部链接

**Local Installation**:
从本地文件导入、与 Marketplace Installation 分开管理的 Installed Skill；相同名称或内容不会使它被并入市场安装。
_Avoid_: Marketplace Installation、自动更新 Skill

**Identical Skills**:
同一管理类别内，两份 Skill 的相对文件路径集合以及每个文件的规范化内容均完全一致；UTF-8 文本的 CRLF/LF 和末尾一个换行不影响判同，二进制仍逐字节比较。仅同名、描述相似或部分文件相同都不属于 Identical Skills。不同来源的远端内容变化不改变已有 Installation Lineage。
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
由用户组织的一组 Skill 集合。
_Avoid_: 场景、安装批次
