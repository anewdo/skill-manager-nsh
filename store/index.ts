import { setColorScheme, signal, type Signal } from "nsh";
import type { ToastMessage, ToastTone } from "../components/ui";
import * as api from "../lib/api";
import { conflictSkillId, describeError, toAppError } from "../lib/errors";
import { translate, type MessageKey, type MessageParams } from "../i18n";
import type {
  AppError,
  InstalledSkill,
  InstallResult,
  OperationEvent,
  OperationLabel,
  OperationSnapshot,
  Settings,
  SettingsPatch,
  SkillSource,
} from "../lib/types";
import { createDownloads, type Downloads } from "./downloads";
import { createToasts } from "./toasts";

/** One marketplace install the app has started and still tracks. */
export interface InstallTracker {
  operationId: string;
  sourceKey: string;
  installedBefore: string[];
}

export type TextFn = (key: MessageKey, params?: MessageParams) => string;

export interface AppStore {
  readonly settings: Signal<Settings | null>;
  readonly settingsError: Signal<AppError | null>;
  /** Every Installed Skill of the library. */
  readonly skills: Signal<InstalledSkill[]>;
  readonly skillsError: Signal<AppError | null>;
  /** The operation running or queued most recently, of any kind. */
  readonly activeOperation: Signal<OperationSnapshot | null>;
  readonly installs: Signal<InstallTracker[]>;
  readonly toasts: Signal<ToastMessage[]>;
  readonly syncError: Signal<AppError | null>;
  readonly downloads: Downloads;
  /** A message in the language of the settings; reactive. */
  readonly text: TextFn;
  /** An error as a sentence in the language of the settings. */
  errorText(error: AppError): string;
  start(): void;
  refreshSkills(): Promise<void>;
  saveSettings(patch: SettingsPatch): Promise<Settings>;
  /** The latest snapshot of an operation the app has seen. */
  operation(id: string): OperationSnapshot | null;
  startInstall(source: SkillSource, label: OperationLabel): void;
  cancelOperation(id: string): void;
  notify(tone: ToastTone, message: string): void;
  dismissToast(id: string): void;
  /** Hold the toasts' timeouts while the stack is expanded under the pointer. */
  setToastsExpanded(expanded: boolean): void;
  /** Escape or the back button: dismiss the newest toast. */
  dismissNewestToast(): void;
}

function isActive(snapshot: OperationSnapshot): boolean {
  return snapshot.state === "queued" || snapshot.state === "running";
}

/** The app's shared state: settings, the skill library and operations. */
export function createAppStore(): AppStore {
  const settings = signal<Settings | null>(null);
  const settingsError = signal<AppError | null>(null);
  const skills = signal<InstalledSkill[]>([]);
  const skillsError = signal<AppError | null>(null);
  const activeOperation = signal<OperationSnapshot | null>(null);
  const operations = signal<Record<string, OperationSnapshot>>({});
  const installs = signal<InstallTracker[]>([]);
  const syncError = signal<AppError | null>(null);
  const toasts = createToasts();
  const downloads = createDownloads(operations);

  let started = false;
  let skillsRequest = 0;
  let settingsQueue: Promise<unknown> = Promise.resolve();

  const text: TextFn = (key, params) =>
    translate(settings()?.language ?? "zh_cn")(key, params);

  function errorText(error: AppError): string {
    return describeError(error, translate(settings()?.language ?? "zh_cn"));
  }

  function notifyFailure(prefix: MessageKey, error: unknown): void {
    toasts.push("error", `${text(prefix)}: ${errorText(toAppError(error))}`);
  }

  function remember(snapshot: OperationSnapshot): void {
    operations.set({ ...operations(), [snapshot.id]: snapshot });
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
          downloads.forget(snapshot.id);
          settleInstall(snapshot);
          if (activeOperation()?.id === snapshot.id) {
            activeOperation.set(null);
          }
        }
        return;
      case "operation-progress":
        if (snapshot) {
          downloads.sample(snapshot);
          if (isActive(snapshot)) {
            activeOperation.set(snapshot);
          }
        }
        return;
      case "library-changed":
        void refreshSkills();
        return;
      case "operations-resync":
        void resync();
        return;
    }
  }

  function readActiveOperation(): void {
    try {
      const current = api.activeOperation();
      if (current) {
        remember(current);
      }
      activeOperation.set(current !== null && isActive(current) ? current : null);
      syncError.set(null);
    } catch (error) {
      syncError.set(toAppError(error));
    }
  }

  async function resync(): Promise<void> {
    readActiveOperation();
    await refreshSkills();
  }

  async function refreshSkills(): Promise<void> {
    const request = ++skillsRequest;
    try {
      const list = await api.listSkills();
      if (request !== skillsRequest) {
        return;
      }
      skills.set(list);
      skillsError.set(null);
    } catch (error) {
      if (request !== skillsRequest) {
        return;
      }
      skillsError.set(toAppError(error));
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

  function start(): void {
    if (started) {
      return;
    }
    started = true;
    void api.subscribeOperations(handleEvent).catch((error) => {
      syncError.set(toAppError(error));
    });
    downloads.start();
    void loadSettings();
    void refreshSkills();
    readActiveOperation();
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
      toasts.push("info", text("install.cancelled"));
      return;
    }
    if (snapshot.state === "failed") {
      if (snapshot.error === null) {
        toasts.push("error", text("install.failed"));
        return;
      }
      const error = toAppError(snapshot.error);
      const conflictId = conflictSkillId(error);
      if (conflictId === null) {
        notifyFailure("install.failed", error);
        return;
      }
      const existing = skills().find((item) => item.id === conflictId);
      toasts.push(
        "warning",
        text("install.conflict", { name: existing?.display_name ?? conflictId }),
      );
      return;
    }
    if (snapshot.state !== "succeeded") {
      return;
    }
    const result = snapshot.result as InstallResult | null;
    const name = result?.display_name ?? "";
    const reused = result !== null && tracker.installedBefore.includes(result.id);
    if (result !== null) {
      skills.set([...skills().filter((skill) => skill.id !== result.id), result]);
    }
    void refreshSkills();
    toasts.push(
      "success",
      reused ? text("install.reused", { name }) : text("install.succeeded", { name }),
    );
    if (result?.unmanaged_copy) {
      toasts.push(
        "info",
        text("install.unmanagedCopy", {
          copy: result.unmanaged_copy,
          installed: result.root_relpath,
        }),
      );
    }
  }

  function startInstall(source: SkillSource, label: OperationLabel): void {
    const installedBefore = skills().map((skill) => skill.id);
    try {
      const operationId = api.installFromSource(source, label);
      installs.set([
        ...installs(),
        { operationId, sourceKey: source.source_key, installedBefore },
      ]);
    } catch (error) {
      notifyFailure("install.failed", error);
    }
  }

  function cancelOperation(id: string): void {
    try {
      api.cancelOperation(id);
    } catch (error) {
      notifyFailure("operation.cancelFailed", error);
    }
  }

  return {
    settings,
    settingsError,
    skills,
    skillsError,
    activeOperation,
    installs,
    toasts: toasts.list,
    syncError,
    downloads,
    text,
    errorText,
    start,
    refreshSkills,
    saveSettings,
    operation,
    startInstall,
    cancelOperation,
    notify: toasts.push,
    dismissToast: toasts.dismiss,
    setToastsExpanded: toasts.setExpanded,
    dismissNewestToast: toasts.dismissNewest,
  };
}
