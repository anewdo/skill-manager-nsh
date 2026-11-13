import type { ImportCandidate, ImportCandidateOrigin } from "../../lib/types";
import type { TextFn } from "../../store";

/** The display names of the Agent tools whose skill directories are scanned. */
const agentNames: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex / ChatGPT",
  cursor: "Cursor",
  gemini: "Gemini CLI",
  copilot: "GitHub Copilot",
  windsurf: "Windsurf",
  opencode: "OpenCode",
};

/** What the badge of a candidate says about where it was found. */
export function candidateOriginLabel(origin: ImportCandidateOrigin, text: TextFn): string {
  switch (origin.kind) {
    case "agent":
      return agentNames[origin.agent] ?? origin.agent;
    case "custom":
      return text("import.origin.custom");
    case "repository":
      return origin.repository;
  }
}

/** The directory a candidate sits in, on either separator. */
function pathRoot(path: string): string {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index > 0 ? path.slice(0, index) : path;
}

export interface ImportGroup {
  key: string;
  label: string;
  /** The distinct directories the group's candidates sit in. */
  roots: string[];
  candidates: ImportCandidate[];
}

/** Gathers candidates by the tool or folder that holds them, first seen first. */
export function groupCandidates(
  candidates: readonly ImportCandidate[],
  text: TextFn,
): ImportGroup[] {
  const groups = new Map<string, ImportGroup>();
  for (const candidate of candidates) {
    const key =
      candidate.origin.kind === "agent"
        ? `agent-${candidate.origin.agent}`
        : candidate.origin.kind === "repository"
          ? `repository-${candidate.origin.repository}`
          : "custom";
    const root = pathRoot(candidate.path);
    const group = groups.get(key);
    if (group === undefined) {
      groups.set(key, {
        key,
        label: candidateOriginLabel(candidate.origin, text),
        roots: [root],
        candidates: [candidate],
      });
    } else {
      group.candidates.push(candidate);
      if (!group.roots.includes(root)) {
        group.roots.push(root);
      }
    }
  }
  return [...groups.values()];
}

export interface RepositoryAddress {
  repository: string;
  /** The folder or SKILL.md path the address points at, when it has one. */
  path: string | null;
}

const namePart = /^[A-Za-z0-9_.-]+$/;

function repositoryOf(owner: string | undefined, repo: string | undefined): RepositoryAddress | null {
  if (
    owner === undefined ||
    repo === undefined ||
    !namePart.test(owner) ||
    !namePart.test(repo)
  ) {
    return null;
  }
  return { repository: `${owner}/${repo.replace(/\.git$/i, "")}`, path: null };
}

function parseGithubPath(path: string): RepositoryAddress | null {
  const parts = path.split("/").filter((part) => part !== "");
  const base = repositoryOf(parts[0], parts[1]);
  if (base === null) {
    return null;
  }
  const mode = parts[2];
  if ((mode === "tree" || mode === "blob") && parts.length >= 5) {
    return { repository: base.repository, path: parts.slice(4).join("/") };
  }
  return base;
}

/** Reads a GitHub address the way the import field accepts it; null when it is not one. */
export function parseRepositoryAddress(input: string): RepositoryAddress | null {
  const value = input.trim().replace(/^https?:\/\//i, "").replace(/^www\./i, "");
  if (value === "") {
    return null;
  }
  if (/^github\.com\//i.test(value)) {
    return parseGithubPath(value.slice("github.com/".length));
  }
  const parts = value.split("/");
  return parts.length === 2 ? repositoryOf(parts[0], parts[1]) : null;
}
