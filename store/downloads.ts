import { computed, signal, type Signal } from "nsh";
import type { OperationSnapshot } from "../lib/types";

/** How far back the displayed transfer rate looks. */
const SPEED_WINDOW_MS = 1000;
/** A transfer silent this long shows no rate. */
const SPEED_STALE_MS = 3000;
const SPEED_TICK_MS = 500;

export interface Downloads {
  /** 安装与本地导入任务，依次显示运行中、排队中和失败项。 */
  readonly queue: () => OperationSnapshot[];
  /** Transferred bytes per second of the operations a sample pair covers. */
  readonly speeds: Signal<Record<string, number>>;
  /** Starts refreshing the rates; call once. */
  start(): void;
  /** Records the transferred bytes a progress event reported. */
  sample(snapshot: OperationSnapshot): void;
  /** Drops the samples of an operation that ended. */
  forget(id: string): void;
  /** 从队列隐藏失败任务。 */
  dismiss(id: string): void;
  /** 从队列隐藏全部失败任务。 */
  clearFailed(): void;
}

function sameRates(left: Record<string, number>, right: Record<string, number>): boolean {
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every((key) => right[key] === left[key]);
}

/** The download queue derived from the operations the app has seen. */
export function createDownloads(
  operations: () => Record<string, OperationSnapshot>,
): Downloads {
  /** 用户已关闭的失败任务。 */
  const dismissed = signal<Record<string, true>>({});
  const speeds = signal<Record<string, number>>({});
  const samples = new Map<string, { at: number; bytes: number }[]>();

  const queue = computed<OperationSnapshot[]>(() => {
    const hidden = dismissed();
    const tasks = Object.values(operations()).filter(
      (snapshot) => snapshot.kind === "install" || snapshot.kind === "import_local" || snapshot.kind === "import_url",
    );
    const active = tasks
      .filter((snapshot) => snapshot.state === "queued" || snapshot.state === "running")
      .sort((left, right) => {
        if (left.state !== right.state) {
          return left.state === "running" ? -1 : 1;
        }
        return left.created_at - right.created_at;
      });
    const failed = tasks
      .filter((snapshot) => snapshot.state === "failed" && hidden[snapshot.id] !== true)
      .sort((left, right) => left.updated_at - right.updated_at);
    return [...active, ...failed];
  });

  function sample(snapshot: OperationSnapshot): void {
    const bytes = snapshot.progress?.bytes;
    if (bytes === undefined) {
      return;
    }
    const now = Date.now();
    const list = samples.get(snapshot.id) ?? [];
    list.push({ at: now, bytes });
    const cutoff = now - SPEED_WINDOW_MS * 2;
    while (list.length > 0 && list[0].at < cutoff) {
      list.shift();
    }
    samples.set(snapshot.id, list);
  }

  function refreshSpeeds(): void {
    const now = Date.now();
    const next: Record<string, number> = {};
    for (const [id, list] of samples) {
      const recent = list.filter((item) => now - item.at <= SPEED_WINDOW_MS);
      if (recent.length < 2) {
        continue;
      }
      const first = recent[0];
      const last = recent[recent.length - 1];
      const elapsed = last.at - first.at;
      if (elapsed > 0 && now - last.at <= SPEED_STALE_MS) {
        next[id] = Math.max(0, ((last.bytes - first.bytes) * 1000) / elapsed);
      }
    }
    if (!sameRates(next, speeds())) {
      speeds.set(next);
    }
  }

  function forget(id: string): void {
    samples.delete(id);
    if (speeds()[id] !== undefined) {
      const next = { ...speeds() };
      delete next[id];
      speeds.set(next);
    }
  }

  function dismiss(id: string): void {
    dismissed.set({ ...dismissed(), [id]: true });
  }

  function clearFailed(): void {
    const next = { ...dismissed() };
    for (const snapshot of queue()) {
      if (snapshot.state === "failed") {
        next[snapshot.id] = true;
      }
    }
    dismissed.set(next);
  }

  return {
    queue,
    speeds,
    start: () => {
      setInterval(refreshSpeeds, SPEED_TICK_MS);
    },
    sample,
    forget,
    dismiss,
    clearFailed,
  };
}
