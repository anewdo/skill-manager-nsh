import { expect, test } from "nsh/test";
import type { Installation, MarketplaceSkill, SourceType } from "../../types";
import {
  GRID_COLUMNS,
  PAGE_SIZE,
  chunk,
  clampPage,
  filterBySource,
  formatCount,
  installedLineages,
  installationById,
  pageCount,
  pageRange,
  paginate,
  skillBody,
} from "./model";

function skill(name: string, sourceType: SourceType = "github"): MarketplaceSkill {
  return {
    id: `${sourceType}:owner/repo/${name}`,
    skill_id: name,
    name,
    source: "owner/repo",
    installs: 10,
    lineage: {
      source_type: sourceType,
      source_key: `${sourceType}:owner/repo/${name}`,
      repository: "owner/repo",
      subpath: name,
    },
  };
}

function installation(id: string, sourceKey: string | null): Installation {
  return {
    id,
    category: "marketplace",
    display_name: id,
    root_relpath: id,
    root_key: id,
    source: sourceKey === null
      ? null
      : { source_type: "github", source_key: sourceKey },
    installed_revision: null,
    remote_revision: null,
    baseline_hash: null,
    current_hash: null,
    local_state: "clean",
    update_state: "unknown",
    update_checked_at: null,
    created_at: 0,
    updated_at: 0,
    last_error: null,
  };
}

test("filters the entries by source type", () => {
  const entries = [skill("a"), skill("b", "well_known"), skill("c")];

  expect(filterBySource(entries, "all")).toEqual(entries);
  expect(filterBySource(entries, "github").map((entry) => entry.name)).toEqual(["a", "c"]);
  expect(filterBySource(entries, "well_known").map((entry) => entry.name)).toEqual(["b"]);
  expect(filterBySource(entries, "all")).not.toBe(entries);
});

test("counts pages and keeps the current one inside them", () => {
  expect(pageCount(0)).toBe(1);
  expect(pageCount(PAGE_SIZE)).toBe(1);
  expect(pageCount(PAGE_SIZE + 1)).toBe(2);
  expect(pageCount(49, 24)).toBe(3);

  expect(clampPage(0, 3)).toBe(1);
  expect(clampPage(2, 3)).toBe(2);
  expect(clampPage(9, 3)).toBe(3);
  expect(clampPage(9, 0)).toBe(1);
});

test("pages through the entries 24 at a time", () => {
  const entries = Array.from({ length: 30 }, (_, index) => index);

  expect(paginate(entries, 1)).toHaveLength(24);
  expect(paginate(entries, 1)[0]).toBe(0);
  expect(paginate(entries, 2)).toEqual([24, 25, 26, 27, 28, 29]);
  expect(paginate(entries, 3)).toEqual([]);
});

test("reports the range a page shows", () => {
  expect(pageRange(1, 0)).toBeNull();
  expect(pageRange(1, 30)).toEqual({ start: 1, end: 24 });
  expect(pageRange(2, 30)).toEqual({ start: 25, end: 30 });
  expect(pageRange(9, 30)).toEqual({ start: 25, end: 30 });
  expect(pageRange(1, 5)).toEqual({ start: 1, end: 5 });
});

test("chunks entries into rows of the grid", () => {
  const rows = chunk(Array.from({ length: 7 }, (_, index) => index));

  expect(GRID_COLUMNS).toBe(3);
  expect(rows).toHaveLength(3);
  expect(rows[0]).toEqual([0, 1, 2]);
  expect(rows[2]).toEqual([6]);
  expect(chunk([], 3)).toEqual([]);
});

test("collects the lineage keys of the installations that have a source", () => {
  const keys = installedLineages([
    installation("one", "github:owner/repo/one"),
    installation("two", "well-known:example.com/two"),
    installation("local", null),
  ]);

  expect(keys.has("github:owner/repo/one")).toBe(true);
  expect(keys.has("well-known:example.com/two")).toBe(true);
  expect(keys.size).toBe(2);
});

test("finds an installation by id", () => {
  const entries = [installation("one", "key"), installation("two", "key")];

  expect(installationById(entries, "two")?.id).toBe("two");
  expect(installationById(entries, null)).toBeNull();
  expect(installationById(entries, "missing")).toBeNull();
});

test("formats install counts with thousands separators", () => {
  expect(formatCount(0)).toBe("0");
  expect(formatCount(999)).toBe("999");
  expect(formatCount(1234)).toBe("1,234");
  expect(formatCount(1234567)).toBe("1,234,567");
  expect(formatCount(-5)).toBe("0");
});

test("drops the YAML frontmatter of a skill document", () => {
  expect(skillBody("---\nname: demo\ndescription: Demo\n---\n# Demo\nBody\n")).toBe(
    "# Demo\nBody\n",
  );
  expect(skillBody("---\r\nname: demo\r\n---\r\n# Demo\r\n")).toBe("# Demo\n");
  expect(skillBody("# No frontmatter\n")).toBe("# No frontmatter\n");
  expect(skillBody("---\nname: unclosed\n")).toBe("---\nname: unclosed\n");
});
