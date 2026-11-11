import type { Installation, OperationProgress, SkillSource } from "../../lib/types";

export const PAGE_SIZE = 24;
export const CARD_COLUMNS = 3;

export type ViewMode = "list" | "card";

export const viewModes: ViewMode[] = ["list", "card"];

export type StatusFilter = "all" | "clean" | "modified" | "missing" | "update_available";

export const statusFilters: StatusFilter[] = [
  "all",
  "clean",
  "modified",
  "missing",
  "update_available",
];

export function isUpdateAvailable(installation: Installation): boolean {
  return installation.update_state === "available";
}

export function matchesFilter(installation: Installation, filter: StatusFilter): boolean {
  if (filter === "all") {
    return true;
  }
  if (filter === "update_available") {
    return isUpdateAvailable(installation);
  }
  return installation.local_state === filter;
}

function matchesQuery(installation: Installation, term: string): boolean {
  if (term === "") {
    return true;
  }
  const values = [
    installation.display_name,
    installation.root_relpath,
    installation.source?.source_key,
    installation.source?.repository,
  ];
  return values.some((value) => value !== undefined && value.toLowerCase().includes(term));
}

export function filterInstallations(
  installations: Installation[],
  query: string,
  filter: StatusFilter,
): Installation[] {
  const term = query.trim().toLowerCase();
  return installations.filter(
    (installation) => matchesFilter(installation, filter) && matchesQuery(installation, term),
  );
}

export function countForFilter(installations: Installation[], filter: StatusFilter): number {
  return installations.filter((installation) => matchesFilter(installation, filter)).length;
}

export function selectedInstallations(
  installations: Installation[],
  selected: ReadonlySet<string>,
): Installation[] {
  return installations.filter((installation) => selected.has(installation.id));
}

export function allSelected(visibleIds: string[], selected: ReadonlySet<string>): boolean {
  return visibleIds.length > 0 && visibleIds.every((id) => selected.has(id));
}

export function someSelected(visibleIds: string[], selected: ReadonlySet<string>): boolean {
  return !allSelected(visibleIds, selected) && visibleIds.some((id) => selected.has(id));
}

export function chunkItems<T>(items: readonly T[], size: number = CARD_COLUMNS): T[][] {
  const rows: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    rows.push(items.slice(index, index + size));
  }
  return rows;
}

export function sourceLabel(source: SkillSource | null): string {
  if (source === null) {
    return "";
  }
  return source.repository ?? source.source_key;
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) {
    return "—";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

export function progressPercent(progress: OperationProgress | null): number | undefined {
  if (!progress || progress.current === undefined || progress.total === undefined) {
    return undefined;
  }
  if (progress.total <= 0) {
    return undefined;
  }
  return Math.min(100, Math.max(0, (progress.current / progress.total) * 100));
}
