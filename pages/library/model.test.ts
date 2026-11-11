import { expect, test } from "nsh/test";
import type { InstalledSkill, LocalState, UpdateState } from "../../lib/types";
import {
  allSelected,
  cardColumnsFor,
  filterCounts,
  filterSkills,
  selectedSkills,
  skillById,
  someSelected,
  sourceLabel,
} from "./model";

function skill(
  id: string,
  overrides: {
    localState?: LocalState;
    updateState?: UpdateState;
    repository?: string;
    sourceKey?: string;
  } = {},
): InstalledSkill {
  return {
    id,
    display_name: id,
    root_relpath: `scope/${id}`,
    root_key: `scope/${id}`,
    source: {
      source_type: "github",
      source_key: overrides.sourceKey ?? `github:owner/repo/${id}`,
      repository: overrides.repository ?? "owner/repo",
      subpath: id,
    },
    installed_revision: null,
    remote_revision: null,
    baseline_hash: null,
    current_hash: null,
    local_state: overrides.localState ?? "clean",
    update_state: overrides.updateState ?? "unknown",
    update_checked_at: null,
    created_at: 0,
    updated_at: 0,
    last_error: null,
  };
}

const ids = (skills: InstalledSkill[]): string[] => skills.map((item) => item.id);

test("filters skills by local state and update availability", () => {
  const entries = [
    skill("one"),
    skill("two", { localState: "modified" }),
    skill("three", { localState: "missing" }),
    skill("four", { updateState: "available" }),
  ];

  expect(filterSkills(entries, "", "all")).toHaveLength(4);
  expect(ids(filterSkills(entries, "", "clean"))).toEqual(["one", "four"]);
  expect(ids(filterSkills(entries, "", "modified"))).toEqual(["two"]);
  expect(ids(filterSkills(entries, "", "missing"))).toEqual(["three"]);
  expect(ids(filterSkills(entries, "", "update_available"))).toEqual(["four"]);
});

test("searches names, paths and sources case-insensitively", () => {
  const entries = [
    skill("Alpha", { repository: "owner/alpha-tools", sourceKey: "github:owner/alpha" }),
    skill("beta", { repository: "owner/beta-tools", sourceKey: "github:owner/beta" }),
  ];

  expect(ids(filterSkills(entries, "ALPHA", "all"))).toEqual(["Alpha"]);
  expect(ids(filterSkills(entries, "owner/beta", "all"))).toEqual(["beta"]);
  expect(ids(filterSkills(entries, "scope/alpha", "all"))).toEqual(["Alpha"]);
  expect(filterSkills(entries, "  ", "all")).toHaveLength(2);
  expect(filterSkills(entries, "missing", "all")).toHaveLength(0);
});

test("combines the search term with the status filter", () => {
  const entries = [skill("one", { localState: "modified" }), skill("two")];

  expect(ids(filterSkills(entries, "one", "modified"))).toEqual(["one"]);
  expect(filterSkills(entries, "two", "modified")).toHaveLength(0);
});

test("counts every filter from the whole library", () => {
  const counts = filterCounts([
    skill("one"),
    skill("two", { localState: "modified" }),
    skill("three", { updateState: "available" }),
    skill("four", { localState: "unknown" }),
  ]);

  expect(counts).toEqual({ all: 4, clean: 2, modified: 1, missing: 0, update_available: 1 });
});

test("keeps the selected skills in list order", () => {
  const entries = [skill("one"), skill("two"), skill("three")];

  expect(ids(selectedSkills(entries, new Set(["three", "one"])))).toEqual(["one", "three"]);
  expect(selectedSkills(entries, new Set())).toHaveLength(0);
});

test("reports select-all state for the skills listed", () => {
  const all = new Set(["one", "two"]);
  const partial = new Set(["one"]);

  expect(allSelected(["one", "two"], all)).toBe(true);
  expect(allSelected(["one"], all)).toBe(true);
  expect(allSelected([], all)).toBe(false);
  expect(someSelected(["one", "two"], partial)).toBe(true);
  expect(someSelected(["one", "two"], all)).toBe(false);
  expect(someSelected(["one", "two"], new Set())).toBe(false);
});

test("finds a skill by id", () => {
  const entries = [skill("one"), skill("two")];

  expect(skillById(entries, "two")?.id).toBe("two");
  expect(skillById(entries, null)).toBeNull();
  expect(skillById(entries, "missing")).toBeNull();
});

test("labels the source with its repository", () => {
  expect(sourceLabel(skill("one").source)).toBe("owner/repo");
  expect(sourceLabel(null)).toBe("");
  expect(sourceLabel({ source_type: "well_known", source_key: "well-known:example.com/x" })).toBe(
    "well-known:example.com/x",
  );
});

test("picks the card columns from the window width", () => {
  expect(cardColumnsFor(800)).toBe(2);
  expect(cardColumnsFor(1024)).toBe(3);
  expect(cardColumnsFor(1400)).toBe(4);
});
