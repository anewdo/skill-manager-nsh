import { effect, signal } from "nsh";
import * as api from "../../lib/api";
import { toAppError } from "../../lib/errors";
import type {
  AppError,
  ConnectivityCheck,
  Language,
  ProxyMode,
  SettingsPatch,
  Theme,
} from "../../lib/types";
import type { AppStore } from "../../store";

export type SaveStatus = "idle" | "saving" | "saved";

export type SettingsController = ReturnType<typeof createSettingsController>;

/** The settings page's state. Every change is saved at once and undone if saving fails. */
export function createSettingsController(store: AppStore) {
  const text = store.text;

  const language = signal<Language>("zh_cn");
  const theme = signal<Theme>("system");
  const proxyMode = signal<ProxyMode>("system");
  const customUrl = signal("");
  const status = signal<SaveStatus>("idle");
  const saveError = signal<string | null>(null);

  const dataDir = signal<string | null>(null);
  const dataDirError = signal<AppError | null>(null);

  const testBusy = signal(false);
  const testChecks = signal<ConnectivityCheck[]>([]);
  const testError = signal<AppError | null>(null);

  // The controls show the saved settings whenever those change.
  effect(() => {
    const current = store.settings();
    if (current === null) {
      return;
    }
    language.set(current.language);
    theme.set(current.theme);
    proxyMode.set(current.proxy.mode);
    customUrl.set(current.proxy.custom_url ?? "");
  });

  async function persist(patch: SettingsPatch): Promise<boolean> {
    status.set("saving");
    saveError.set(null);
    try {
      await store.saveSettings(patch);
      status.set("saved");
      return true;
    } catch (error) {
      status.set("idle");
      saveError.set(`${text("settings.saveFailed")}: ${store.errorText(toAppError(error))}`);
      return false;
    }
  }

  async function setLanguage(value: Language): Promise<void> {
    const previous = language();
    language.set(value);
    if (!(await persist({ language: value }))) {
      language.set(previous);
    }
  }

  async function setTheme(value: Theme): Promise<void> {
    const previous = theme();
    theme.set(value);
    if (!(await persist({ theme: value }))) {
      theme.set(previous);
    }
  }

  function clearTest(): void {
    testChecks.set([]);
    testError.set(null);
  }

  function setProxyMode(value: ProxyMode): void {
    proxyMode.set(value);
    clearTest();
    saveError.set(null);
    if (value !== "custom") {
      void persist({ proxy: { mode: value } });
    } else if (customUrl().trim() !== "") {
      void persist({ proxy: { mode: value, custom_url: customUrl().trim() } });
    }
  }

  function editCustomUrl(value: string): void {
    customUrl.set(value);
    saveError.set(null);
  }

  /** Saves the custom proxy address when it differs from the saved one. */
  async function saveCustomUrl(): Promise<boolean> {
    if (proxyMode() !== "custom") {
      return true;
    }
    const url = customUrl().trim();
    if (url === "") {
      saveError.set(text("settings.proxy.required"));
      return false;
    }
    const saved = store.settings()?.proxy;
    const normalized = url.includes("://") ? url : `http://${url}`;
    if (saved?.mode === "custom" && saved.custom_url === normalized) {
      return true;
    }
    return await persist({ proxy: { mode: "custom", custom_url: url } });
  }

  async function testConnectivity(): Promise<void> {
    if (testBusy() || !(await saveCustomUrl())) {
      return;
    }
    testBusy.set(true);
    clearTest();
    try {
      testChecks.set(await api.testConnectivity());
    } catch (error) {
      testError.set(toAppError(error));
    } finally {
      testBusy.set(false);
    }
  }

  async function loadDataDirectory(): Promise<void> {
    dataDirError.set(null);
    try {
      dataDir.set(await api.dataDirectory());
    } catch (error) {
      dataDirError.set(toAppError(error));
    }
  }

  void loadDataDirectory();

  return {
    language,
    theme,
    proxyMode,
    customUrl,
    status,
    saveError,
    dismissSaveError: () => saveError.set(null),
    setLanguage,
    setTheme,
    setProxyMode,
    editCustomUrl,
    saveCustomUrl,
    testBusy,
    testChecks,
    testError,
    testConnectivity,
    dataDir,
    dataDirError,
    loadDataDirectory,
  };
}
