import type { InstalledSkill, MarketplaceSkill, SourceType } from "../../lib/types";

export const SEARCH_LIMIT = 60;

export type SourceFilter = "all" | SourceType;

export const sourceFilters: SourceFilter[] = ["all", "github", "well_known"];

export function filterBySource(
  skills: readonly MarketplaceSkill[],
  filter: SourceFilter,
): MarketplaceSkill[] {
  if (filter === "all") {
    return [...skills];
  }
  return skills.filter((skill) => skill.lineage.source_type === filter);
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

/** The lineage keys of the Installed Skills whose files still exist. */
export function installedLineages(skills: readonly InstalledSkill[]): Set<string> {
  const keys = new Set<string>();
  for (const skill of skills) {
    if (skill.source && skill.local_state !== "missing") {
      keys.add(skill.source.source_key);
    }
  }
  return keys;
}

/** Where a Marketplace Entry stands for the user: what its install button says and does. */
export type EntryStatus = "available" | "queued" | "installing" | "installed";
