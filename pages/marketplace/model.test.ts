import { expect, test } from "nsh/test";
import type { InstalledSkill, MarketplaceSkill, SourceType } from "../../lib/types";
import { filterBySource, githubOwner, gridColumnsFor, installedLineages } from "./model";

function entry(name: string, sourceType: SourceType = "github"): MarketplaceSkill {
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

function skill(id: string, sourceKey: string | null): InstalledSkill {
  return {
    id,
    display_name: id,
    root_relpath: id,
    root_key: id,
    source: sourceKey === null ? null : { source_type: "github", source_key: sourceKey },
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
  const entries = [entry("a"), entry("b", "well_known"), entry("c")];

  expect(filterBySource(entries, "all")).toEqual(entries);
  expect(filterBySource(entries, "github").map((item) => item.name)).toEqual(["a", "c"]);
  expect(filterBySource(entries, "well_known").map((item) => item.name)).toEqual(["b"]);
  expect(filterBySource(entries, "all")).not.toBe(entries);
});

test("collects the lineage keys of the skills whose files still exist", () => {
  const missing = skill("gone", "github:owner/repo/gone");
  missing.local_state = "missing";
  const keys = installedLineages([
    skill("one", "github:owner/repo/one"),
    skill("two", "well-known:example.com/two"),
    skill("local", null),
    missing,
  ]);

  expect([...keys].sort()).toEqual(["github:owner/repo/one", "well-known:example.com/two"]);
});

test("picks the grid columns from the window width", () => {
  expect(gridColumnsFor(600)).toBe(1);
  expect(gridColumnsFor(700)).toBe(2);
  expect(gridColumnsFor(1024)).toBe(3);
  expect(gridColumnsFor(1400)).toBe(4);
});

test("takes the owner from a GitHub source and none from a website source", () => {
  expect(githubOwner(entry("a"))).toBe("owner");

  const website = entry("b", "well_known");
  website.source = "example.com";
  expect(githubOwner(website)).toBeNull();
});
