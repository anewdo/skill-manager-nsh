import { expect, test } from "nsh/test";
import { translate } from "../i18n";
import { formatBytes, formatCount, progressPercent, skillBody } from "./format";
import { isCancellable, operationDetail, operationTitle } from "./operation-text";
import type { OperationSnapshot } from "./types";

const t = translate("en");

function snapshot(overrides: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    id: "op",
    kind: "install",
    state: "running",
    cancel_requested: false,
    progress: null,
    result: null,
    error: null,
    created_at: 0,
    updated_at: 0,
    ...overrides,
  };
}

test("formats counts with thousands separators", () => {
  expect(formatCount(0)).toBe("0");
  expect(formatCount(999)).toBe("999");
  expect(formatCount(1234567)).toBe("1,234,567");
  expect(formatCount(-5)).toBe("0");
});

test("formats byte counts with binary units", () => {
  expect(formatBytes(null)).toBe("—");
  expect(formatBytes(0)).toBe("0 B");
  expect(formatBytes(1023)).toBe("1023 B");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(12 * 1024)).toBe("12 KB");
  expect(formatBytes(3.5 * 1024 * 1024)).toBe("3.5 MB");
  expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GB");
});

test("turns progress into a percentage", () => {
  expect(progressPercent(null)).toBeUndefined();
  expect(progressPercent({ phase: "checking", cancellable: true })).toBeUndefined();
  expect(progressPercent({ phase: "checking", cancellable: true, current: 1, total: 4 })).toBe(25);
  expect(progressPercent({ phase: "checking", cancellable: true, current: 9, total: 4 })).toBe(100);
  expect(progressPercent({ phase: "checking", cancellable: true, current: 0, total: 0 })).toBeUndefined();
});

test("drops the YAML frontmatter of a skill document", () => {
  expect(skillBody("---\nname: demo\ndescription: Demo\n---\n# Demo\nBody\n")).toBe("# Demo\nBody\n");
  expect(skillBody("---\r\nname: demo\r\n---\r\n# Demo\r\n")).toBe("# Demo\n");
  expect(skillBody("# No frontmatter\n")).toBe("# No frontmatter\n");
  expect(skillBody("---\nname: unclosed\n")).toBe("---\nname: unclosed\n");
});

test("titles an install with its skill and other operations with what they do", () => {
  expect(operationTitle(t, snapshot({ label: { name: "demo", source: "owner/repo" } }))).toBe("demo");
  expect(operationTitle(t, snapshot())).toBe("Unnamed skill");
  expect(operationTitle(t, snapshot({ kind: "check_updates" }))).toBe("Checking for updates");
});

test("describes where an operation is", () => {
  expect(operationDetail(t, snapshot({ state: "queued" }))).toBe("Waiting");
  expect(operationDetail(t, snapshot({ cancel_requested: true }))).toBe("Cancelling…");
  expect(operationDetail(t, snapshot())).toBe("Working…");
  expect(
    operationDetail(
      t,
      snapshot({
        state: "failed",
        error: { code: "NETWORK", message: "timed out", retryable: true },
      }),
    ),
  ).toBe("timed out");
  expect(
    operationDetail(
      t,
      snapshot({
        progress: {
          phase: "downloading",
          cancellable: true,
          current: 1,
          total: 4,
          bytes: 2048,
          bytes_total: 4096,
          detail: "Receiving objects (1/4)",
        },
      }),
      1024,
    ),
  ).toBe("Downloading · 25% · Receiving Git objects 1/4 · 1.0 KB/s · 2.0 KB / 4.0 KB");
  expect(
    operationDetail(
      t,
      snapshot({
        kind: "check_updates",
        progress: { phase: "checking", cancellable: true, current: 2, total: 5 },
      }),
    ),
  ).toBe("Checking · 2/5");
});

test("lets the user cancel only what can still stop", () => {
  expect(isCancellable(snapshot({ state: "queued" }))).toBe(true);
  expect(isCancellable(snapshot({ progress: { phase: "downloading", cancellable: true } }))).toBe(true);
  expect(isCancellable(snapshot({ progress: { phase: "installing", cancellable: false } }))).toBe(false);
  expect(isCancellable(snapshot({ state: "queued", cancel_requested: true }))).toBe(false);
  expect(isCancellable(snapshot({ state: "failed" }))).toBe(false);
});
