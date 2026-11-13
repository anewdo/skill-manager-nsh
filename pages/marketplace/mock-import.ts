import type { ImportCandidate } from "../../lib/types";

function agentSkill(
  path: string,
  name: string,
  agent: string,
  description: string,
): ImportCandidate {
  return { id: path, name, description, origin: { kind: "agent", agent }, path };
}

const agentSkills: ImportCandidate[] = [
  agentSkill(
    "~/.claude/skills/pdf",
    "pdf",
    "claude",
    "Extract text and tables from PDF files, fill forms and merge documents.",
  ),
  agentSkill(
    "~/.claude/skills/pptx",
    "pptx",
    "claude",
    "Create and edit slide decks from an outline.",
  ),
  agentSkill(
    "~/.claude/skills/release-notes",
    "release-notes",
    "claude",
    "Turn merged pull requests into release notes.",
  ),
  agentSkill(
    "~/.codex/skills/spreadsheet",
    "spreadsheet",
    "codex",
    "Analyze and transform spreadsheet files.",
  ),
  agentSkill(
    "~/.cursor/skills/code-review",
    "code-review",
    "cursor",
    "Review a diff for correctness, clarity and risk.",
  ),
  agentSkill(
    "~/.gemini/skills/doc-writer",
    "doc-writer",
    "gemini",
    "Turn rough notes into a structured document.",
  ),
];

/** The Agent directories' skills, minus the ones the library already manages. */
export function mockAgentCandidates(managedNames: ReadonlySet<string>): ImportCandidate[] {
  return agentSkills.filter((candidate) => !managedNames.has(candidate.name));
}

/** Skills a folder pick would return. */
export function mockFolderCandidates(): ImportCandidate[] {
  return [
    {
      id: "D:\\team-skills\\ui-review",
      name: "ui-review",
      description: "Review a screen against the design tokens and report the gaps.",
      origin: { kind: "custom" },
      path: "D:\\team-skills\\ui-review",
    },
    {
      id: "D:\\team-skills\\api-contract",
      name: "api-contract",
      description: "Compare an implementation with its OpenAPI contract.",
      origin: { kind: "custom" },
      path: "D:\\team-skills\\api-contract",
    },
  ];
}

/** Skills a ZIP pick would return. */
export function mockZipCandidates(): ImportCandidate[] {
  return [
    {
      id: "D:\\Downloads\\design-skills.zip#design-audit",
      name: "design-audit",
      description: "Check screenshots against spacing, type and color rules.",
      origin: { kind: "custom" },
      path: "D:\\Downloads\\design-skills.zip\\design-audit",
    },
  ];
}

/** Skills a repository discovery would return. */
export function mockRepositoryCandidates(repository: string): ImportCandidate[] {
  const entries: Array<[string, string, string]> = [
    ["pdf", "skills/pdf", "Extract text and tables from PDF files, fill forms and merge documents."],
    ["pptx", "skills/pptx", "Create and edit slide decks from an outline."],
    ["spreadsheet", "skills/spreadsheet", "Analyze and transform spreadsheet files."],
    ["code-review", "skills/engineering/code-review", "Review a diff for correctness, clarity and risk."],
  ];
  return entries.map(([name, path, description]) => ({
    id: `${repository}#${path}`,
    name,
    description,
    origin: { kind: "repository", repository },
    path,
  }));
}
