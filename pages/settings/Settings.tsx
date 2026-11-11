import { computed, effect, signal } from "nsh";
import { Scroll, Text, View } from "nsh";
import { Input, Tab, Tabs } from "nsh/base";
import * as api from "../../lib/api";
import { Icon, type IconName } from "../../components/icon";
import { ActionButton, Banner } from "../../components/ui";
import { describeError, toAppError } from "../../lib/errors";
import { translate, type MessageKey } from "../../i18n";
import type { AppStore } from "../../store";
import type {
  AppError,
  ConnectivityCheck,
  ConnectivityTarget,
  Language,
  ProxyMode,
  SettingsPatch,
  Theme,
} from "../../lib/types";

const languages: Language[] = ["zh_cn", "en"];
const themes: Theme[] = ["light", "dark", "system"];
const proxyModes: ProxyMode[] = ["system", "none", "custom"];

const languageKeys: Record<Language, MessageKey> = {
  zh_cn: "settings.language.zh_cn",
  en: "settings.language.en",
};

const themeKeys: Record<Theme, MessageKey> = {
  light: "settings.theme.light",
  dark: "settings.theme.dark",
  system: "settings.theme.system",
};

const proxyModeKeys: Record<ProxyMode, MessageKey> = {
  system: "settings.proxy.mode.system",
  none: "settings.proxy.mode.none",
  custom: "settings.proxy.mode.custom",
};

const targetKeys: Record<ConnectivityTarget, MessageKey> = {
  github: "settings.proxy.target.github",
  skills_sh: "settings.proxy.target.skills_sh",
};

const tabClass = "group rounded-(--radius) px-2.5 py-1 active:bg-accent selected:bg-secondary";
const tabTextClass = "text-xs text-muted-foreground group-selected:text-foreground";
const sectionClass = "gap-3 rounded-(--radius) border border-border bg-card p-5";
const rowLabelClass = "w-[120px] text-sm text-foreground";
const sectionTitleClass = "text-sm font-semibold text-foreground";
const sectionHintClass = "text-xs text-muted-foreground";

function SectionTitle(props: { icon: IconName; label: string }) {
  return (
    <View class="flex-row items-center gap-2">
      <Icon name={props.icon} color="foreground" />
      <Text class={sectionTitleClass}>{props.label}</Text>
    </View>
  );
}

export function SettingsPage(props: { store: AppStore }) {
  const text = (key: MessageKey, params?: Record<string, string | number>): string =>
    translate(props.store.settings()?.language ?? "zh_cn")(key, params);

  const language = signal<Language>("zh_cn");
  const theme = signal<Theme>("system");
  const proxyMode = signal<ProxyMode>("system");
  const customUrl = signal("");
  const status = signal<"idle" | "saving" | "saved">("idle");
  const saveError = signal<string | null>(null);

  const dataDir = signal<string | null>(null);
  const dataDirLoading = signal(false);
  const dataDirError = signal<AppError | null>(null);

  const testBusy = signal(false);
  const testChecks = signal<ConnectivityCheck[]>([]);
  const testError = signal<AppError | null>(null);

  const statusText = computed(() => {
    switch (status()) {
      case "saving":
        return text("settings.status.saving");
      case "saved":
        return text("settings.status.saved");
      default:
        return text("settings.status.auto");
    }
  });

  const errorText = (error: AppError): string =>
    describeError(error, translate(props.store.settings()?.language ?? "zh_cn"));

  effect(() => {
    const current = props.store.settings();
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
      await props.store.saveSettings(patch);
      status.set("saved");
      return true;
    } catch (error) {
      status.set("idle");
      saveError.set(`${text("settings.saveFailed")}: ${errorText(toAppError(error))}`);
      return false;
    }
  }

  async function saveCustomUrl(): Promise<boolean> {
    if (proxyMode() !== "custom") {
      return true;
    }
    const url = customUrl().trim();
    if (url === "") {
      saveError.set(text("settings.proxy.required"));
      return false;
    }
    const current = props.store.settings();
    const saved = current !== null && current.proxy.mode === "custom"
      ? (current.proxy.custom_url ?? "")
      : null;
    const normalized = url.includes("://") ? url : `http://${url}`;
    if (saved !== null && saved === normalized) {
      return true;
    }
    return await persist({ proxy: { mode: "custom", custom_url: url } });
  }

  async function editLanguage(value: Language): Promise<void> {
    if (value === language()) {
      return;
    }
    const previous = language();
    language.set(value);
    saveError.set(null);
    if (!(await persist({ language: value }))) {
      language.set(previous);
    }
  }

  async function editTheme(value: Theme): Promise<void> {
    if (value === theme()) {
      return;
    }
    const previous = theme();
    theme.set(value);
    saveError.set(null);
    if (!(await persist({ theme: value }))) {
      theme.set(previous);
    }
  }

  function editProxyMode(value: ProxyMode): void {
    if (value === proxyMode()) {
      return;
    }
    proxyMode.set(value);
    testChecks.set([]);
    testError.set(null);
    saveError.set(null);
    if (value === "custom" && customUrl().trim() === "") {
      return;
    }
    const patch: SettingsPatch = {
      proxy:
        value === "custom"
          ? { mode: value, custom_url: customUrl().trim() }
          : { mode: value },
    };
    void persist(patch);
  }

  async function runConnectivity(): Promise<void> {
    if (testBusy()) {
      return;
    }
    if (!(await saveCustomUrl())) {
      return;
    }
    testBusy.set(true);
    testError.set(null);
    testChecks.set([]);
    try {
      testChecks.set(await api.testConnectivity());
    } catch (error) {
      testError.set(toAppError(error));
    } finally {
      testBusy.set(false);
    }
  }

  async function loadDataDirectory(): Promise<void> {
    dataDirLoading.set(true);
    dataDirError.set(null);
    try {
      dataDir.set(await api.dataDirectory());
    } catch (error) {
      dataDirError.set(toAppError(error));
    } finally {
      dataDirLoading.set(false);
    }
  }

  void loadDataDirectory();

  return (
    <View class="flex-1 bg-background">
      <View class="gap-3 border-b border-border bg-card px-6 py-4">
        <View class="flex-row items-center gap-3">
          <View class="flex-1 gap-1">
            <Text class="text-lg font-semibold text-foreground">{text("settings.title")}</Text>
            <Text class="text-xs text-muted-foreground">{text("settings.subtitle")}</Text>
          </View>
          <Text class="text-xs text-muted-foreground" testID="settings-save-status">
            {statusText()}
          </Text>
        </View>
        <If when={saveError()}>
          {(message) => (
            <Banner
              tone="error"
              message={message()}
              actionLabel={text("settings.dismiss")}
              onAction={() => saveError.set(null)}
              testID="settings-save-error"
            />
          )}
        </If>
      </View>

      <Scroll class="flex-1">
        <View class="max-w-[720px] gap-4 p-6">
          <View class={sectionClass}>
            <SectionTitle icon="palette" label={text("settings.appearance.title")} />
            <View class="flex-row items-center gap-3">
              <Text class={rowLabelClass}>{text("settings.language")}</Text>
              <Tabs class="flex-row gap-1">
                <Each of={languages}>
                  {(item) => (
                    <Tab
                      selected={item() === language()}
                      onPress={() => void editLanguage(item())}
                      class={tabClass}
                      testID={`settings-language-${item()}`}
                    >
                      <Text class={tabTextClass}>{text(languageKeys[item()])}</Text>
                    </Tab>
                  )}
                </Each>
              </Tabs>
            </View>
            <View class="flex-row items-center gap-3">
              <Text class={rowLabelClass}>{text("settings.theme")}</Text>
              <Tabs class="flex-row gap-1">
                <Each of={themes}>
                  {(item) => (
                    <Tab
                      selected={item() === theme()}
                      onPress={() => void editTheme(item())}
                      class={tabClass}
                      testID={`settings-theme-${item()}`}
                    >
                      <Text class={tabTextClass}>{text(themeKeys[item()])}</Text>
                    </Tab>
                  )}
                </Each>
              </Tabs>
            </View>
          </View>

          <View class={sectionClass}>
            <SectionTitle icon="globe" label={text("settings.proxy.title")} />
            <View class="flex-row items-center gap-3">
              <Text class={rowLabelClass}>{text("settings.proxy.mode")}</Text>
              <Tabs class="flex-row gap-1">
                <Each of={proxyModes}>
                  {(item) => (
                    <Tab
                      selected={item() === proxyMode()}
                      onPress={() => editProxyMode(item())}
                      class={tabClass}
                      testID={`settings-proxy-${item()}`}
                    >
                      <Text class={tabTextClass}>{text(proxyModeKeys[item()])}</Text>
                    </Tab>
                  )}
                </Each>
              </Tabs>
            </View>
            <If when={proxyMode() === "custom"}>
              {() => (
                <View class="gap-2">
                  <Text class="text-sm text-foreground">{text("settings.proxy.url")}</Text>
                  <Input
                    class="w-[420px] rounded-(--radius) border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
                    value={customUrl()}
                    placeholder={text("settings.proxy.urlPlaceholder")}
                    submitBehavior="submit"
                    onChange={(event) => {
                      customUrl.set(event.value);
                      saveError.set(null);
                    }}
                    onBlur={() => void saveCustomUrl()}
                    onSubmit={() => void saveCustomUrl()}
                    testID="settings-proxy-url"
                  />
                  <Text class={sectionHintClass}>{text("settings.proxy.urlHint")}</Text>
                </View>
              )}
            </If>
            <View class="flex-row items-center gap-3">
              <ActionButton
                label={testBusy() ? text("settings.proxy.testing") : text("settings.proxy.test")}
                variant="secondary"
                disabled={testBusy()}
                onPress={() => void runConnectivity()}
                testID="settings-test-connectivity"
              />
              <Text class={sectionHintClass}>{text("settings.proxy.testHint")}</Text>
            </View>
            <If when={testError()}>
              {(error) => (
                <Text class="text-sm text-destructive" testID="settings-connectivity-error">
                  {`${text("settings.proxy.test.error")}: ${errorText(error())}`}
                </Text>
              )}
            </If>
            <Each of={testChecks()}>
              {(item) => (
                <View
                  class="flex-row items-center gap-3"
                  testID={`settings-connectivity-${item().target}`}
                >
                  <Text class="w-[80px] text-sm text-foreground">
                    {text(targetKeys[item().target])}
                  </Text>
                  <Text class={item().ok ? "text-sm text-foreground" : "text-sm text-destructive"}>
                    {item().ok
                      ? text("settings.proxy.test.ok", { latency: item().latency_ms })
                      : `${text("settings.proxy.test.fail")}${item().error ? `: ${item().error}` : ""}`}
                  </Text>
                </View>
              )}
            </Each>
          </View>

          <View class={sectionClass}>
            <View class="gap-1">
              <SectionTitle icon="folder" label={text("settings.data.title")} />
              <Text class={sectionHintClass}>{text("settings.data.hint")}</Text>
            </View>
            <If when={dataDirLoading()}>
              {() => <Text class="text-sm text-muted-foreground">{text("common.loading")}</Text>}
            </If>
            <If when={dataDirError()}>
              {(error) => (
                <View class="flex-row items-center gap-2">
                  <Text class="flex-1 text-sm text-destructive">
                    {`${text("settings.data.error")}: ${errorText(error())}`}
                  </Text>
                  <ActionButton
                    label={text("common.retry")}
                    variant="secondary"
                    onPress={() => void loadDataDirectory()}
                  />
                </View>
              )}
            </If>
            <If when={dataDir() !== null && dataDirError() === null}>
              {() => (
                <Text class="text-sm text-foreground" testID="settings-data-directory">
                  {dataDir() ?? ""}
                </Text>
              )}
            </If>
          </View>
        </View>
      </Scroll>
    </View>
  );
}
