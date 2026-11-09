import type { Language } from "./types";

const zh = {
  "app.title": "Skills Manager",
  "app.stateError": "应用状态同步失败",
  "nav.marketplace": "市场",

  "common.retry": "重试",
  "common.refresh": "刷新",
  "common.cancel": "取消",
  "common.close": "关闭",
  "common.loading": "加载中…",
  "common.search": "搜索",
  "common.clear": "清除",

  "marketplace.searchPlaceholder": "搜索 skills.sh",
  "marketplace.leaderboard.all_time": "总榜",
  "marketplace.leaderboard.trending": "趋势",
  "marketplace.leaderboard.hot": "热门",
  "marketplace.sourceFilter.label": "来源",
  "marketplace.sourceFilter.all": "全部来源",
  "marketplace.sourceFilter.github": "GitHub",
  "marketplace.sourceFilter.well_known": "网站",
  "marketplace.resultCount": "共 {count} 条",
  "marketplace.loading": "正在加载市场数据…",
  "marketplace.searching": "正在搜索…",
  "marketplace.empty": "暂无 Skills",
  "marketplace.emptyHint": "换个榜单或稍后重试。",
  "marketplace.searchEmpty": "没有匹配的 Skills",
  "marketplace.filteredEmpty": "当前来源筛选下没有结果",
  "marketplace.clearFilter": "显示全部来源",
  "marketplace.error.title": "市场数据加载失败",
  "marketplace.card.installs": "{count} 次安装",
  "marketplace.card.installed": "已安装",
  "marketplace.card.preview": "预览",
  "marketplace.card.install": "安装",
  "marketplace.card.installQueued": "排队中",
  "marketplace.card.installing": "安装中",
  "marketplace.preview.title": "Skill 预览",
  "marketplace.preview.loading": "正在下载并读取 SKILL.md…",
  "marketplace.preview.error": "预览失败",
  "marketplace.preview.retry": "重新预览",
  "marketplace.preview.noDescription": "该 Skill 没有描述。",
  "marketplace.install.queued": "已排队，等待开始",
  "marketplace.install.downloading": "正在下载",
  "marketplace.install.validating": "正在校验内容",
  "marketplace.install.installing": "正在写入中央仓库",
  "marketplace.install.cancel": "取消安装",
  "marketplace.install.cancelled": "安装已取消",
  "marketplace.install.succeeded": "已安装「{name}」",
  "marketplace.install.reused": "已有相同内容，已复用安装「{name}」",
  "marketplace.install.failed": "安装失败",
  "marketplace.install.conflict": "该来源已安装为「{name}」，更新请前往 My Skills",
  "marketplace.install.dismiss": "知道了",
  "marketplace.pagination.label": "分页",
  "marketplace.pagination.summary": "第 {current} / {total} 页",
  "marketplace.range": "显示 {start}-{end}，共 {total} 条",
  "marketplace.rangeEmpty": "没有可显示的条目",

  "error.invalid_input": "请求无效",
  "error.unsafe_path": "路径不安全",
  "error.database": "数据存储失败",
  "error.io": "文件操作失败",
  "error.not_found": "未找到",
  "error.conflict": "存在冲突",
  "error.network": "网络请求失败",
  "error.git": "Git 操作失败",
  "error.invalid_skill": "Skill 内容无效",
  "error.ambiguous_skill": "下载内容包含多个 Skill",
  "error.local_modified": "本地文件已修改",
  "error.cancelled": "操作已取消",
  "error.unsupported": "当前不支持",
  "error.internal": "内部错误",
};

export type MessageKey = keyof typeof zh;

const en: Record<MessageKey, string> = {
  "app.title": "Skills Manager",
  "app.stateError": "Could not sync app state",
  "nav.marketplace": "Marketplace",

  "common.retry": "Retry",
  "common.refresh": "Refresh",
  "common.cancel": "Cancel",
  "common.close": "Close",
  "common.loading": "Loading…",
  "common.search": "Search",
  "common.clear": "Clear",

  "marketplace.searchPlaceholder": "Search skills.sh",
  "marketplace.leaderboard.all_time": "All time",
  "marketplace.leaderboard.trending": "Trending",
  "marketplace.leaderboard.hot": "Hot",
  "marketplace.sourceFilter.label": "Source",
  "marketplace.sourceFilter.all": "All sources",
  "marketplace.sourceFilter.github": "GitHub",
  "marketplace.sourceFilter.well_known": "Websites",
  "marketplace.resultCount": "{count} results",
  "marketplace.loading": "Loading the marketplace…",
  "marketplace.searching": "Searching…",
  "marketplace.empty": "No skills yet",
  "marketplace.emptyHint": "Try another ranking or retry later.",
  "marketplace.searchEmpty": "No matching skills",
  "marketplace.filteredEmpty": "No results for this source filter",
  "marketplace.clearFilter": "Show all sources",
  "marketplace.error.title": "Could not load the marketplace",
  "marketplace.card.installs": "{count} installs",
  "marketplace.card.installed": "Installed",
  "marketplace.card.preview": "Preview",
  "marketplace.card.install": "Install",
  "marketplace.card.installQueued": "Queued",
  "marketplace.card.installing": "Installing",
  "marketplace.preview.title": "Skill preview",
  "marketplace.preview.loading": "Downloading and reading SKILL.md…",
  "marketplace.preview.error": "Preview failed",
  "marketplace.preview.retry": "Preview again",
  "marketplace.preview.noDescription": "This skill has no description.",
  "marketplace.install.queued": "Queued, waiting to start",
  "marketplace.install.downloading": "Downloading",
  "marketplace.install.validating": "Validating the files",
  "marketplace.install.installing": "Installing into the skills repository",
  "marketplace.install.cancel": "Cancel install",
  "marketplace.install.cancelled": "Installation cancelled",
  "marketplace.install.succeeded": "Installed {name}",
  "marketplace.install.reused": "The same content was already installed as {name}",
  "marketplace.install.failed": "Installation failed",
  "marketplace.install.conflict": "Already installed as {name}; update it in My Skills",
  "marketplace.install.dismiss": "Dismiss",
  "marketplace.pagination.label": "Pagination",
  "marketplace.pagination.summary": "Page {current} of {total}",
  "marketplace.range": "Showing {start}-{end} of {total}",
  "marketplace.rangeEmpty": "Nothing to show",

  "error.invalid_input": "Invalid request",
  "error.unsafe_path": "Unsafe path",
  "error.database": "Storage failure",
  "error.io": "File operation failed",
  "error.not_found": "Not found",
  "error.conflict": "Conflict",
  "error.network": "Network request failed",
  "error.git": "Git operation failed",
  "error.invalid_skill": "Invalid skill",
  "error.ambiguous_skill": "The download contains several skills",
  "error.local_modified": "Local files were modified",
  "error.cancelled": "Cancelled",
  "error.unsupported": "Not supported",
  "error.internal": "Internal error",
};

export type MessageParams = Record<string, string | number>;
export type Translate = (key: MessageKey, params?: MessageParams) => string;

const tables: Record<Language, Record<MessageKey, string>> = { zh_cn: zh, en };

export function translate(language: Language): Translate {
  const table = tables[language];
  return (key, params) => {
    const template = table[key];
    if (!params) {
      return template;
    }
    return template.replace(/\{(\w+)\}/g, (match, name: string) => {
      const value = params[name];
      return value === undefined ? match : String(value);
    });
  };
}

export function messageKeys(): MessageKey[] {
  return Object.keys(zh) as MessageKey[];
}
