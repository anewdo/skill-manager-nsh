import type {
  AppError,
  ImportCandidate,
  LocalDiscovery,
  LocalImportReport,
  LocalImportSource,
} from "../../lib/types";
import type { TextFn } from "../../store";

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

export interface LocalImportRow {
  id: string;
  group?: ImportGroup;
  candidate?: ImportCandidate;
  issue?: { path: string; error: AppError };
  source?: LocalImportSource;
}

export function localImportRows(
  groups: ImportGroup[],
  collapsed: ReadonlySet<string>,
  discovery: LocalDiscovery,
): LocalImportRow[] {
  const rows: LocalImportRow[] = [];
  for (const group of groups) {
    if (group.candidates.length === 1) {
      const candidate = group.candidates[0];
      rows.push({ id: candidate.id, candidate });
      continue;
    }
    rows.push({ id: group.key, group });
    if (!collapsed.has(group.key)) {
      rows.push(...group.candidates.map((candidate) => ({ id: candidate.id, candidate })));
    }
  }
  for (const source of discovery.sources) {
    if (source.status === "empty" || source.status === "invalid") {
      rows.push({ id: `source-${source.id}`, source });
    }
  }
  discovery.issues.forEach((issue, index) => rows.push({ id: `issue-${index}`, issue }));
  return rows;
}

export function summarizeImport(report: LocalImportReport, text: TextFn): string {
  const counts = { added: 0, existing: 0, failed: 0, cancelled: 0 };
  for (const item of report.items) counts[item.status] += 1;
  return text("import.local.summary", counts);
}

/** 按本地来源分组，保留发现顺序。 */
export function groupCandidates(
  candidates: readonly ImportCandidate[],
  text: TextFn,
): ImportGroup[] {
  const groups = new Map<string, ImportGroup>();
  for (const candidate of candidates) {
    const key = `${candidate.origin.kind}-${candidate.origin.source_id}`;
    const root = pathRoot(candidate.path);
    const group = groups.get(key);
    if (group === undefined) {
      groups.set(key, {
        key,
        label: text("import.origin.custom"),
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
