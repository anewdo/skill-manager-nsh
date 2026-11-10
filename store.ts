import { setColorScheme, signal, type Signal } from "nsh";
import * as api from "./api";
import { toAppError } from "./errors";
import type {
  AppError,
  Installation,
  OperationEvent,
  OperationSnapshot,
  Settings,
  SettingsPatch,
} from "./types";

export interface AppStore {
  readonly settings: Signal<Settings | null>;
  readonly settingsError: Signal<AppError | null>;
  readonly installations: Signal<Installation[]>;
  readonly installationsError: Signal<AppError | null>;
  readonly activeOperation: Signal<OperationSnapshot | null>;
  readonly operations: Signal<Record<string, OperationSnapshot>>;
  readonly syncError: Signal<AppError | null>;
  start(): void;
  refreshInstallations(): Promise<void>;
  saveSettings(patch: SettingsPatch): Promise<Settings>;
  operation(id: string): OperationSnapshot | null;
}

/** The app's shared state: settings, marketplace installations and operations. */
export function createAppStore(): AppStore {
  const settings = signal<Settings | null>(null);
  const settingsError = signal<AppError | null>(null);
  const installations = signal<Installation[]>([]);
  const installationsError = signal<AppError | null>(null);
  const activeOperation = signal<OperationSnapshot | null>(null);
  const operations = signal<Record<string, OperationSnapshot>>({});
  const syncError = signal<AppError | null>(null);

  let started = false;
  let installationsRequest = 0;
  let settingsQueue: Promise<unknown> = Promise.resolve();

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
        } else if (activeOperation()?.id === snapshot.id) {
          activeOperation.set(null);
        }
        return;
      case "operation-progress":
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
      const list = await api.listInstallations("marketplace");
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

  return {
    settings,
    settingsError,
    installations,
    installationsError,
    activeOperation,
    operations,
    syncError,
    start,
    refreshInstallations,
    saveSettings,
    operation,
  };
}
