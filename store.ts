import { computed, setColorScheme, signal, type Signal } from "nsh";
import * as api from "./lib/api";
import { conflictInstallationId, describeError, toAppError } from "./lib/errors";
import { translate, type MessageKey, type MessageParams } from "./i18n";
import type {
  AppError,
  Installation,
  InstallResult,
  OperationEvent,
  OperationLabel,
  OperationSnapshot,
  Settings,
  SettingsPatch,
  SkillSource,
} from "./lib/types";

/** One marketplace install the app has started and still tracks. */
export interface InstallTracker {
  operationId: string;
  sourceKey: string;
  installedBefore: string[];
}

export type ToastTone = "success" | "error" | "info";

export interface ToastMessage {
  id: number;
  tone: ToastTone;
  message: string;
}

export interface AppStore {
  readonly settings: Signal<Settings | null>;
  readonly settingsError: Signal<AppError | null>;
  readonly installations: Signal<Installation[]>;
  readonly installationsError: Signal<AppError | null>;
  readonly activeOperation: Signal<OperationSnapshot | null>;
  readonly operations: Signal<Record<string, OperationSnapshot>>;
  readonly installs: Signal<InstallTracker[]>;
  readonly toasts: Signal<ToastMessage[]>;
  readonly syncError: Signal<AppError | null>;
  /** Installs the download queue shows: the running one, then queued, then failed. */
  readonly installQueue: () => OperationSnapshot[];
  /** Transferred bytes per second of the operations a sample pair covers. */
  readonly installSpeeds: Signal<Record<string, number>>;
  start(): void;
  refreshInstallations(): Promise<void>;
  saveSettings(patch: SettingsPatch): Promise<Settings>;
  operation(id: string): OperationSnapshot | null;
  startInstall(source: SkillSource, label: OperationLabel): void;
  cancelInstall(id: string): void;
  dismissInstall(id: string): void;
  clearFailedInstalls(): void;
  dismissToast(id: number): void;
}

const TOAST_DURATION_MS = 4500;
const MAX_TOASTS = 3;
/** How far back the displayed transfer rate looks. */
const SPEED_WINDOW_MS = 1000;
/** A transfer silent this long shows no rate. */
const SPEED_STALE_MS = 3000;
const SPEED_TICK_MS = 500;

/** The app's shared state: settings, marketplace installations and operations. */
export function createAppStore(): AppStore {
  const settings = signal<Settings | null>(null);
  const settingsError = signal<AppError | null>(null);
  const installations = signal<Installation[]>([]);
  const installationsError = signal<AppError | null>(null);
  const activeOperation = signal<OperationSnapshot | null>(null);
  const operations = signal<Record<string, OperationSnapshot>>({});
  const installs = signal<InstallTracker[]>([]);
  const toasts = signal<ToastMessage[]>([]);
  const syncError = signal<AppError | null>(null);
  /** Failed installs the user closed, hidden from the queue from then on. */
  const dismissedInstalls = signal<Record<string, true>>({});
  /** Transferred bytes per second of each operation the samples cover. */
  const installSpeeds = signal<Record<string, number>>({});
  const byteSamples = new Map<string, { at: number; bytes: number }[]>();

  let started = false;
  let installationsRequest = 0;
  let settingsQueue: Promise<unknown> = Promise.resolve();
  let toastSequence = 0;
  const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

  const installQueue = computed<OperationSnapshot[]>(() => {
    const dismissed = dismissedInstalls();
    const installsByKind = Object.values(operations()).filter(
      (snapshot) => snapshot.kind === "install",
    );
    const active = installsByKind
      .filter((snapshot) => snapshot.state === "queued" || snapshot.state === "running")
      .sort((left, right) => {
        if (left.state !== right.state) {
          return left.state === "running" ? -1 : 1;
        }
        return left.created_at - right.created_at;
      });
    const failed = installsByKind
      .filter((snapshot) => snapshot.state === "failed" && dismissed[snapshot.id] !== true)
      .sort((left, right) => left.updated_at - right.updated_at);
    return [...active, ...failed];
  });

  function sampleBytes(snapshot: OperationSnapshot): void {
    const bytes = snapshot.progress?.bytes;
    if (bytes === undefined) {
      return;
    }
    const now = Date.now();
    const samples = byteSamples.get(snapshot.id) ?? [];
    samples.push({ at: now, bytes });
    const cutoff = now - SPEED_WINDOW_MS * 2;
    while (samples.length > 0 && samples[0].at < cutoff) {
      samples.shift();
    }
    byteSamples.set(snapshot.id, samples);
  }

  function sameRates(left: Record<string, number>, right: Record<string, number>): boolean {
    const keys = Object.keys(left);
    if (keys.length !== Object.keys(right).length) {
      return false;
    }
    return keys.every((key) => right[key] === left[key]);
  }

  function refreshInstallSpeeds(): void {
    const now = Date.now();
    const next: Record<string, number> = {};
    for (const [id, samples] of byteSamples) {
      const recent = samples.filter((sample) => now - sample.at <= SPEED_WINDOW_MS);
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
    if (!sameRates(next, installSpeeds())) {
      installSpeeds.set(next);
    }
  }

  function forgetSamples(id: string): void {
    byteSamples.delete(id);
    if (installSpeeds()[id] !== undefined) {
      const next = { ...installSpeeds() };
      delete next[id];
      installSpeeds.set(next);
    }
  }

  function text(key: MessageKey, params?: MessageParams): string {
    return translate(settings()?.language ?? "zh_cn")(key, params);
  }

  function dropToast(id: number): void {
    const timer = toastTimers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      toastTimers.delete(id);
    }
    toasts.set(toasts().filter((toast) => toast.id !== id));
  }

  function pushToast(tone: ToastTone, message: string): void {
    const id = ++toastSequence;
    const next = [...toasts(), { id, tone, message }];
    while (next.length > MAX_TOASTS) {
      const oldest = next[0];
      const timer = toastTimers.get(oldest.id);
      if (timer !== undefined) {
        clearTimeout(timer);
        toastTimers.delete(oldest.id);
      }
      next.shift();
    }
    toasts.set(next);
    toastTimers.set(
      id,
      setTimeout(() => dropToast(id), TOAST_DURATION_MS),
    );
  }

  function pushFailure(error: AppError): void {
    pushToast(
      "error",
      `${text("marketplace.install.failed")}: ${describeError(
        error,
        translate(settings()?.language ?? "zh_cn"),
      )}`,
    );
  }

  function remember(snapshot: OperationSnapshot): void {
    operations.set({ ...operations(), [snapshot.id]: snapshot });
  }

  function isActive(snapshot: OperationSnapshot): boolean {
    return snapshot.state === "queued" || snapshot.state === "running";
  }

  function handleEvent(event: OperationEvent): void {
    const snapshot = event.operation;
    if (snapshot) {
      remember(snapshot);
    }
    switch (event.kind) {
      case "operation-state":
        if (!snapshot) {
          activeOperation.set(null);
        } else if (isActive(snapshot)) {
          activeOperation.set(snapshot);
        } else {
          forgetSamples(snapshot.id);
          settleInstall(snapshot);
          if (activeOperation()?.id === snapshot.id) {
            activeOperation.set(null);
          }
        }
        return;
      case "operation-progress":
        if (snapshot) {
          sampleBytes(snapshot);
        }
        if (snapshot && isActive(snapshot)) {
          activeOperation.set(snapshot);
        }
        return;
      case "installations-changed":
        void refreshInstallations();
        return;
      case "operations-resync":
        void resync();
        return;
    }
  }

  async function resync(): Promise<void> {
    try {
      const current = api.activeOperation();
      activeOperation.set(current);
      if (current) {
        remember(current);
      }
      syncError.set(null);
    } catch (error) {
      syncError.set(toAppError(error));
    }
    await refreshInstallations();
  }

  async function refreshInstallations(): Promise<void> {
    const request = ++installationsRequest;
    try {
      const list = await api.listInstallations();
      if (request !== installationsRequest) {
        return;
      }
      installations.set(list);
      installationsError.set(null);
    } catch (error) {
      if (request !== installationsRequest) {
        return;
      }
      installationsError.set(toAppError(error));
    }
  }

  async function loadSettings(): Promise<void> {
    try {
      const value = await api.getSettings();
      settings.set(value);
      settingsError.set(null);
      setColorScheme(value.theme);
    } catch (error) {
      settingsError.set(toAppError(error));
    }
  }

  async function restoreActiveOperation(): Promise<void> {
    try {
      const current = api.activeOperation();
      if (current) {
        remember(current);
        if (isActive(current)) {
          activeOperation.set(current);
        }
      }
    } catch (error) {
      syncError.set(toAppError(error));
    }
  }

  function start(): void {
    if (started) {
      return;
    }
    started = true;
    void api.subscribeOperations(handleEvent).catch((error) => {
      syncError.set(toAppError(error));
    });
    setInterval(refreshInstallSpeeds, SPEED_TICK_MS);
    void loadSettings();
    void refreshInstallations();
    void restoreActiveOperation();
  }

  /**
   * Applies a partial settings patch and takes effect at once.
   *
   * Patches are serialized so rapid auto-save changes cannot overwrite each
   * other with an older snapshot.
   */
  async function saveSettings(patch: SettingsPatch): Promise<Settings> {
    const request = settingsQueue.then(() => api.updateSettings(patch));
    settingsQueue = request.catch(() => undefined);
    const value = await request;
    settings.set(value);
    settingsError.set(null);
    setColorScheme(value.theme);
    return value;
  }

  function operation(id: string): OperationSnapshot | null {
    return operations()[id] ?? null;
  }

  /** Reports an install that reached a terminal state and stops tracking it. */
  function settleInstall(snapshot: OperationSnapshot): void {
    const tracker = installs().find((item) => item.operationId === snapshot.id);
    if (tracker === undefined) {
      return;
    }
    installs.set(installs().filter((item) => item.operationId !== snapshot.id));
    if (snapshot.state === "cancelled") {
      pushToast("info", text("marketplace.install.cancelled"));
      return;
    }
    if (snapshot.state === "failed") {
      const error = snapshot.error === null ? null : toAppError(snapshot.error);
      if (error === null) {
        pushToast("error", text("marketplace.install.failed"));
        return;
      }
      const conflictId = conflictInstallationId(error);
      if (conflictId !== null) {
        const existing = installations().find((item) => item.id === conflictId);
        pushToast(
          "error",
          text("marketplace.install.conflict", {
            name: existing?.display_name ?? conflictId,
          }),
        );
        return;
      }
      pushFailure(error);
      return;
    }
    if (snapshot.state !== "succeeded") {
      return;
    }
    const result = snapshot.result as InstallResult | null;
    const name = result?.display_name ?? "";
    const reused = result !== null && tracker.installedBefore.includes(result.id);
    if (result !== null) {
      installations.set([
        ...installations().filter((installation) => installation.id !== result.id),
        result,
      ]);
    }
    void refreshInstallations();
    pushToast(
      "success",
      reused
        ? text("marketplace.install.reused", { name })
        : text("marketplace.install.succeeded", { name }),
    );
    if (result?.unmanaged_copy) {
      pushToast(
        "info",
        text("marketplace.install.unmanagedCopy", {
          copy: result.unmanaged_copy,
          installed: result.root_relpath,
        }),
      );
    }
  }

  function startInstall(source: SkillSource, label: OperationLabel): void {
    const installedBefore = installations().map((installation) => installation.id);
    try {
      const operationId = api.installFromSource(source, label);
      installs.set([
        ...installs(),
        { operationId, sourceKey: source.source_key, installedBefore },
      ]);
    } catch (error) {
      pushFailure(toAppError(error));
    }
  }

  function cancelInstall(id: string): void {
    try {
      api.cancelOperation(id);
    } catch (error) {
      pushFailure(toAppError(error));
    }
  }

  function dismissInstall(id: string): void {
    dismissedInstalls.set({ ...dismissedInstalls(), [id]: true });
  }

  function clearFailedInstalls(): void {
    const next = { ...dismissedInstalls() };
    for (const snapshot of installQueue()) {
      if (snapshot.state === "failed") {
        next[snapshot.id] = true;
      }
    }
    dismissedInstalls.set(next);
  }

  return {
    settings,
    settingsError,
    installations,
    installationsError,
    activeOperation,
    operations,
    installs,
    toasts,
    syncError,
    installQueue,
    installSpeeds,
    start,
    refreshInstallations,
    saveSettings,
    operation,
    startInstall,
    cancelInstall,
    dismissInstall,
    clearFailedInstalls,
    dismissToast: dropToast,
  };
}
