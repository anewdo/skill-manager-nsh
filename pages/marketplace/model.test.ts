import { expect, test } from "nsh/test";
import type { ImportCandidate, InstalledSkill, MarketplaceSkill, SourceType } from "../../lib/types";
import { groupCandidates, parseRepositoryAddress } from "./import-model";
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

test("reads the GitHub addresses the import field accepts", () => {
  expect(parseRepositoryAddress("owner/repo")).toEqual({
    repository: "owner/repo",
    path: null,
  });
  expect(parseRepositoryAddress("https://github.com/owner/repo")).toEqual({
    repository: "owner/repo",
    path: null,
  });
  expect(parseRepositoryAddress("https://www.github.com/owner/repo.git")).toEqual({
    repository: "owner/repo",
    path: null,
  });
  expect(parseRepositoryAddress("https://github.com/owner/repo/tree/main/skills/pdf")).toEqual({
    repository: "owner/repo",
    path: "skills/pdf",
  });
  expect(
    parseRepositoryAddress("https://github.com/owner/repo/blob/main/skills/pdf/SKILL.md"),
  ).toEqual({ repository: "owner/repo", path: "skills/pdf/SKILL.md" });
});

test("rejects addresses that are not GitHub repositories", () => {
  expect(parseRepositoryAddress("")).toBeNull();
  expect(parseRepositoryAddress("not a repository")).toBeNull();
  expect(parseRepositoryAddress("https://gitlab.com/owner/repo")).toBeNull();
  expect(parseRepositoryAddress("github.com/owner")).toBeNull();
});

test("groups import candidates by the tool that holds them", () => {
  const groups = groupCandidates(
    [
      importCandidate("~/.claude/skills/pdf", "pdf", { kind: "agent", agent: "claude" }),
      importCandidate("~/.claude/skills/pptx", "pptx", { kind: "agent", agent: "claude" }),
      importCandidate("~/.codex/skills/sheet", "sheet", { kind: "agent", agent: "codex" }),
      importCandidate("D:\\team\\ui", "ui", { kind: "custom" }),
    ],
    (key) => key,
  );

  expect(groups.map((group) => group.key)).toEqual(["agent-claude", "agent-codex", "custom"]);
  expect(groups[0]?.label).toBe("Claude Code");
  expect(groups[0]?.roots).toEqual(["~/.claude/skills"]);
  expect(groups[0]?.candidates.length).toBe(2);
  expect(groups[2]?.label).toBe("import.origin.custom");
});

function importCandidate(
  path: string,
  name: string,
  origin: ImportCandidate["origin"],
): ImportCandidate {
  return { id: path, name, description: null, origin, path };
}
