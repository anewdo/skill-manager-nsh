import { signal } from "nsh";
import { expect, test } from "nsh/test";
import type { OperationSnapshot } from "../lib/types";
import { createDownloads } from "./downloads";

for (const kind of ["import_local", "import_url"] as const) {
  test(`${kind} remains queued alongside marketplace progress and supports failed dismissal`, () => {
    const install: OperationSnapshot = {
      id: "install",
      kind: "install",
      state: "running",
      cancel_requested: false,
      progress: null,
      result: null,
      error: null,
      created_at: 0,
      updated_at: 0,
    };
    const local: OperationSnapshot = {
      ...install,
      id: "local",
      kind,
      state: "queued",
      created_at: 1,
    };
    const operations = signal<Record<string, OperationSnapshot>>({ local, install });
    const downloads = createDownloads(operations);
    expect(downloads.queue().map((item) => item.id)).toEqual(["install", "local"]);

    operations.set({ local, install: { ...install, updated_at: 2 } });
    expect(downloads.queue().map((item) => item.id)).toEqual(["install", "local"]);

    operations.set({ local: { ...local, state: "failed" }, install });
    expect(downloads.queue().map((item) => item.id)).toEqual(["install", "local"]);
    downloads.dismiss(local.id);
    expect(downloads.queue().map((item) => item.id)).toEqual(["install"]);
  });
}
