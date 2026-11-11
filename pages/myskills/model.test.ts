import { expect, test } from "nsh/test";
import type { Installation, LocalState, UpdateState } from "../../lib/types";
import {
  allSelected,
  chunkItems,
  countForFilter,
  filterInstallations,
  formatBytes,
  progressPercent,
  selectedInstallations,
  someSelected,
  sourceLabel,
} from "./model";

function installation(
  id: string,
  overrides: {
    localState?: LocalState;
    updateState?: UpdateState;
    repository?: string;
    sourceKey?: string;
  } = {},
): Installation {
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

test("filters installations by local state and update availability", () => {
  const entries = [
    installation("one"),
    installation("two", { localState: "modified" }),
    installation("three", { localState: "missing" }),
    installation("four", { updateState: "available" }),
  ];

  expect(filterInstallations(entries, "", "all")).toHaveLength(4);
  expect(filterInstallations(entries, "", "clean").map((item) => item.id)).toEqual([
    "one",
    "four",
  ]);
  expect(filterInstallations(entries, "", "modified").map((item) => item.id)).toEqual(["two"]);
  expect(filterInstallations(entries, "", "missing").map((item) => item.id)).toEqual(["three"]);
  expect(filterInstallations(entries, "", "update_available").map((item) => item.id)).toEqual([
    "four",
  ]);
});

test("searches names, paths and sources case-insensitively", () => {
  const entries = [
    installation("Alpha", { repository: "owner/alpha-tools", sourceKey: "github:owner/alpha" }),
    installation("beta", { repository: "owner/beta-tools", sourceKey: "github:owner/beta" }),
  ];

  expect(filterInstallations(entries, "ALPHA", "all").map((item) => item.id)).toEqual(["Alpha"]);
  expect(filterInstallations(entries, "owner/beta", "all").map((item) => item.id)).toEqual([
    "beta",
  ]);
  expect(filterInstallations(entries, "scope/alpha", "all").map((item) => item.id)).toEqual([
    "Alpha",
  ]);
  expect(filterInstallations(entries, "  ", "all")).toHaveLength(2);
  expect(filterInstallations(entries, "missing", "all")).toHaveLength(0);
});

test("combines the search term with the status filter", () => {
  const entries = [
    installation("one", { localState: "modified" }),
    installation("two", { localState: "clean" }),
  ];

  expect(filterInstallations(entries, "one", "modified").map((item) => item.id)).toEqual(["one"]);
  expect(filterInstallations(entries, "two", "modified")).toHaveLength(0);
});

test("counts every filter from the full list", () => {
  const entries = [
    installation("one"),
    installation("two", { localState: "modified" }),
    installation("three", { updateState: "available" }),
  ];

  expect(countForFilter(entries, "all")).toBe(3);
  expect(countForFilter(entries, "clean")).toBe(2);
  expect(countForFilter(entries, "modified")).toBe(1);
  expect(countForFilter(entries, "update_available")).toBe(1);
});

test("keeps the selected installations in list order", () => {
  const entries = [installation("one"), installation("two"), installation("three")];
  const selected = new Set(["three", "one"]);

  expect(selectedInstallations(entries, selected).map((item) => item.id)).toEqual([
    "one",
    "three",
  ]);
  expect(selectedInstallations(entries, new Set())).toHaveLength(0);
});

test("reports select-all state for the visible page", () => {
  const all = new Set(["one", "two"]);
  const partial = new Set(["one"]);

  expect(allSelected(["one", "two"], all)).toBe(true);
  expect(allSelected(["one"], all)).toBe(true);
  expect(allSelected([], all)).toBe(false);
  expect(someSelected(["one", "two"], partial)).toBe(true);
  expect(someSelected(["one", "two"], all)).toBe(false);
  expect(someSelected(["one", "two"], new Set())).toBe(false);
});

test("labels the source with its repository", () => {
  expect(sourceLabel(installation("one").source)).toBe("owner/repo");
  expect(sourceLabel(null)).toBe("");
  expect(sourceLabel({ source_type: "well_known", source_key: "well-known:example.com/x" })).toBe(
    "well-known:example.com/x",
  );
});

test("formats byte counts with binary units", () => {
  expect(formatBytes(null)).toBe("—");
  expect(formatBytes(0)).toBe("0 B");
  expect(formatBytes(1023)).toBe("1023 B");
  expect(formatBytes(1024)).toBe("1.0 KB");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(12 * 1024)).toBe("12 KB");
  expect(formatBytes(3.5 * 1024 * 1024)).toBe("3.5 MB");
  expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GB");
});

test("turns batch progress into a percentage", () => {
  expect(progressPercent(null)).toBeUndefined();
  expect(progressPercent({ phase: "checking", cancellable: true })).toBeUndefined();
  expect(progressPercent({ phase: "checking", cancellable: true, current: 1, total: 4 })).toBe(25);
  expect(progressPercent({ phase: "checking", cancellable: true, current: 9, total: 4 })).toBe(100);
  expect(progressPercent({ phase: "checking", cancellable: true, current: 0, total: 0 })).toBeUndefined();
});

test("splits the card view into full rows", () => {
  expect(chunkItems([1, 2, 3, 4])).toEqual([[1, 2, 3], [4]]);
  expect(chunkItems([1, 2])).toEqual([[1, 2]]);
  expect(chunkItems([])).toEqual([]);
});
