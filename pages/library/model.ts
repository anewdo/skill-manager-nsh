import type { InstalledSkill, SkillSource } from "../../lib/types";

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

export function isUpdateAvailable(skill: InstalledSkill): boolean {
  return skill.update_state === "available";
}

export function matchesFilter(skill: InstalledSkill, filter: StatusFilter): boolean {
  if (filter === "all") {
    return true;
  }
  if (filter === "update_available") {
    return isUpdateAvailable(skill);
  }
  return skill.local_state === filter;
}

function matchesQuery(skill: InstalledSkill, term: string): boolean {
  if (term === "") {
    return true;
  }
  const values = [
    skill.display_name,
    skill.root_relpath,
    skill.source?.source_key,
    skill.source?.repository,
    skill.source?.git_ref,
    skill.source?.subpath,
  ];
  return values.some((value) => value !== undefined && value.toLowerCase().includes(term));
}

export function filterSkills(
  skills: readonly InstalledSkill[],
  query: string,
  filter: StatusFilter,
): InstalledSkill[] {
  const term = query.trim().toLowerCase();
  return skills.filter((skill) => matchesFilter(skill, filter) && matchesQuery(skill, term));
}

/** How many skills each filter shows, counted from the whole library. */
export function filterCounts(skills: readonly InstalledSkill[]): Record<StatusFilter, number> {
  const counts: Record<StatusFilter, number> = {
    all: skills.length,
    clean: 0,
    modified: 0,
    missing: 0,
    update_available: 0,
  };
  for (const skill of skills) {
    if (skill.local_state !== "unknown") {
      counts[skill.local_state] += 1;
    }
    if (isUpdateAvailable(skill)) {
      counts.update_available += 1;
    }
  }
  return counts;
}

export function selectedSkills(
  skills: readonly InstalledSkill[],
  selected: ReadonlySet<string>,
): InstalledSkill[] {
  return skills.filter((skill) => selected.has(skill.id));
}

export function allSelected(visibleIds: string[], selected: ReadonlySet<string>): boolean {
  return visibleIds.length > 0 && visibleIds.every((id) => selected.has(id));
}

export function someSelected(visibleIds: string[], selected: ReadonlySet<string>): boolean {
  return !allSelected(visibleIds, selected) && visibleIds.some((id) => selected.has(id));
}

export function skillById(
  skills: readonly InstalledSkill[],
  id: string | null,
): InstalledSkill | null {
  if (!id) {
    return null;
  }
  return skills.find((skill) => skill.id === id) ?? null;
}

/** Where a skill came from, in short; empty for a skill without a source. */
export function sourceLabel(source: SkillSource | null): string {
  if (source === null) {
    return "";
  }
  return source.repository ?? source.source_key;
}

/** The card view's column count for the window's width in points. */
export function cardColumnsFor(width: number): number {
  if (width >= 1400) {
    return 4;
  }
  if (width >= 1024) {
    return 3;
  }
  return 2;
}
