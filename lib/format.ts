import type { OperationProgress } from "./types";

/** A whole number with thousands separators; negatives count as 0. */
export function formatCount(value: number): string {
  const text = Math.trunc(Math.max(0, value)).toString();
  return text.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** A byte count in binary units; `null` (not measured) is a dash. */
export function formatBytes(bytes: number | null): string {
  if (bytes === null) {
    return "—";
  }
  if (bytes < 1024) {
    return `${Math.round(bytes)} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

/** How far an operation is, from 0 to 100; `undefined` while its extent is unknown. */
export function progressPercent(progress: OperationProgress | null): number | undefined {
  if (!progress || progress.current === undefined || progress.total === undefined) {
    return undefined;
  }
  if (progress.total <= 0) {
    return undefined;
  }
  return Math.min(100, Math.max(0, (progress.current / progress.total) * 100));
}

/** The SKILL.md body: the YAML frontmatter the app already reports stays out of the document. */
export function skillBody(markdown: string): string {
  const lines = markdown.split(/\r?\n/);
  if (lines[0]?.trimEnd() !== "---") {
    return markdown;
  }
  for (let index = 1; index < lines.length; index += 1) {
    if (lines[index].trimEnd() === "---") {
      return lines.slice(index + 1).join("\n").trimStart();
    }
  }
  return markdown;
}
