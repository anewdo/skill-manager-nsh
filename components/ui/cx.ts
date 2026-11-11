/** Joins class fragments, skipping the ones a condition left out. */
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter((part) => typeof part === "string" && part !== "").join(" ");
}
