import type { Installation, MarketplaceSkill, SourceType } from "../../lib/types";

export const PAGE_SIZE = 24;
export const SEARCH_LIMIT = 60;
export const GRID_COLUMNS = 3;

export type SourceFilter = "all" | SourceType;

export function filterBySource(
  skills: readonly MarketplaceSkill[],
  filter: SourceFilter,
): MarketplaceSkill[] {
  if (filter === "all") {
    return [...skills];
  }
  return skills.filter((skill) => skill.lineage.source_type === filter);
}

export function pageCount(total: number, pageSize: number = PAGE_SIZE): number {
  return Math.max(1, Math.ceil(total / pageSize));
}

export function clampPage(page: number, totalPages: number): number {
  return Math.min(Math.max(1, page), Math.max(1, totalPages));
}

export function paginate<T>(
  items: readonly T[],
  page: number,
  pageSize: number = PAGE_SIZE,
): T[] {
  const start = (page - 1) * pageSize;
  return items.slice(start, start + pageSize);
}

export function chunk<T>(items: readonly T[], size: number = GRID_COLUMNS): T[][] {
  const rows: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    rows.push(items.slice(index, index + size));
  }
  return rows;
}

/** The grid's column count for the window's width in points. */
export function gridColumnsFor(width: number): number {
  if (width >= 1400) {
    return 4;
  }
  if (width >= 1024) {
    return 3;
  }
  if (width >= 700) {
    return 2;
  }
  return 1;
}

/** The lineage keys of the installations the app has and whose files still exist. */
export function installedLineages(installations: readonly Installation[]): Set<string> {
  const keys = new Set<string>();
  for (const installation of installations) {
    if (installation.source && installation.local_state !== "missing") {
      keys.add(installation.source.source_key);
    }
  }
  return keys;
}

export function installationById(
  installations: readonly Installation[],
  id: string | null,
): Installation | null {
  if (!id) {
    return null;
  }
  return installations.find((installation) => installation.id === id) ?? null;
}

/** The SKILL.md body: the YAML frontmatter the preview already reports stays out of the document. */
export function skillBody(markdown: string): string {
  const lines = markdown.split(/\r?\n/);
  if (lines[0]?.trimEnd() !== "---") {
    return markdown;
  }
  for (let index = 1; index < lines.length; index += 1) {
    if (lines[index].trimEnd() === "---") {
      return lines.slice(index + 1).join("\n").trimStart();
    }
  }
  return markdown;
}

/** The items of `page` as a range of `total`, both ends included; `null` when it shows none. */
export function pageRange(
  page: number,
  total: number,
  pageSize: number = PAGE_SIZE,
): { start: number; end: number } | null {
  if (total <= 0) {
    return null;
  }
  const start = (clampPage(page, pageCount(total, pageSize)) - 1) * pageSize + 1;
  return { start, end: Math.min(start + pageSize - 1, total) };
}

export function formatCount(value: number): string {
  const text = Math.trunc(Math.max(0, value)).toString();
  return text.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
