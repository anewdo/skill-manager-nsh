import { Scroll, View } from "nsh";
import { Icon } from "../../components/icon";
import {
  Banner,
  Button,
  Field,
  IconButton,
  Label,
  Notices,
  Page,
  PageHeader,
  Section,
  Segmented,
  TextField,
} from "../../components/ui";
import type { MessageKey } from "../../i18n";
import type { ConnectivityTarget, Language, ProxyMode, Theme } from "../../lib/types";
import type { AppStore } from "../../store";
import { createSettingsController, type SaveStatus } from "./controller";

const languages: Language[] = ["zh_cn", "en"];
const themes: Theme[] = ["system", "light", "dark"];
const proxyModes: ProxyMode[] = ["system", "none", "custom"];

const languageKeys: Record<Language, MessageKey> = {
  zh_cn: "settings.language.zh_cn",
  en: "settings.language.en",
};

const themeKeys: Record<Theme, MessageKey> = {
  system: "settings.theme.system",
  light: "settings.theme.light",
  dark: "settings.theme.dark",
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

const statusKeys: Record<SaveStatus, MessageKey> = {
  idle: "settings.status.auto",
  saving: "settings.status.saving",
  saved: "settings.status.saved",
};

export function SettingsPage(props: { store: AppStore }) {
  const text = props.store.text;
  const c = createSettingsController(props.store);

  return (
    <Page>
      <PageHeader title={text("settings.title")} subtitle={text("settings.subtitle")}>
        <Label variant="caption" tone="muted" testID="settings-save-status">
          {text(statusKeys[c.status()])}
        </Label>
      </PageHeader>

      <If when={c.saveError()}>
        {(message) => (
          <View class="pb-3">
            <Notices>
              <Banner tone="danger" message={message()} testID="settings-save-error">
                <IconButton
                  icon="close"
                  label={text("settings.dismiss")}
                  onPress={c.dismissSaveError}
                />
              </Banner>
            </Notices>
          </View>
        )}
      </If>

      <Scroll class="flex-1">
        <View class="max-w-[720px] gap-4 px-6 pb-6">
          <Section icon="palette" title={text("settings.appearance.title")}>
            <Field label={text("settings.language")}>
              <Segmented
                options={languages}
                value={c.language()}
                label={(option) => text(languageKeys[option])}
                onChange={(option) => void c.setLanguage(option)}
                testID="settings-language"
              />
            </Field>
            <Field label={text("settings.theme")}>
              <Segmented
                options={themes}
                value={c.theme()}
                label={(option) => text(themeKeys[option])}
                onChange={(option) => void c.setTheme(option)}
                testID="settings-theme"
              />
            </Field>
          </Section>

          <Section
            icon="globe"
            title={text("settings.proxy.title")}
            hint={text("settings.proxy.hint")}
          >
            <Field label={text("settings.proxy.mode")}>
              <Segmented
                options={proxyModes}
                value={c.proxyMode()}
                label={(option) => text(proxyModeKeys[option])}
                onChange={c.setProxyMode}
                testID="settings-proxy"
              />
            </Field>
            <If when={c.proxyMode() === "custom"}>
              {() => (
                <Field label={text("settings.proxy.url")}>
                  <View class="flex-1 gap-1.5">
                    <TextField
                      class="w-[320px]"
                      value={c.customUrl()}
                      placeholder={text("settings.proxy.urlPlaceholder")}
                      onChange={c.editCustomUrl}
                      onBlur={() => void c.saveCustomUrl()}
                      onSubmit={() => void c.saveCustomUrl()}
                      testID="settings-proxy-url"
                    />
                    <Label variant="caption" tone="muted">
                      {text("settings.proxy.urlHint")}
                    </Label>
                  </View>
                </Field>
              )}
            </If>
            <View class="flex-row items-center gap-3">
              <Button
                label={text("settings.proxy.test")}
                icon="globe"
                busy={c.testBusy()}
                onPress={() => void c.testConnectivity()}
                testID="settings-test-connectivity"
              />
              <Each of={c.testChecks()} key={(check) => check.target}>
                {(check) => (
                  <View
                    class="flex-row items-center gap-1"
                    testID={`settings-connectivity-${check().target}`}
                  >
                    <Icon
                      name={check().ok ? "circleCheck" : "error"}
                      color={check().ok ? "success" : "destructive"}
                    />
                    <Label variant="caption" tone={check().ok ? "default" : "danger"}>
                      {`${text(targetKeys[check().target])} ${
                        check().ok
                          ? text("settings.proxy.test.ok", { latency: check().latency_ms })
                          : text("settings.proxy.test.fail")
                      }`}
                    </Label>
                  </View>
                )}
              </Each>
            </View>
            <Each of={c.testChecks().filter((check) => !check.ok && check.error !== null)}>
              {(check) => (
                <Label variant="caption" tone="danger" lines={2}>
                  {`${text(targetKeys[check().target])}: ${check().error ?? ""}`}
                </Label>
              )}
            </Each>
            <If when={c.testError()}>
              {(error) => (
                <Label variant="caption" tone="danger" testID="settings-connectivity-error">
                  {`${text("settings.proxy.test.error")}: ${props.store.errorText(error())}`}
                </Label>
              )}
            </If>
          </Section>

          <Section
            icon="folder"
            title={text("settings.data.title")}
            hint={text("settings.data.hint")}
          >
            <If when={c.dataDir()}>
              {(path) => (
                <Label selectable testID="settings-data-directory">
                  {path()}
                </Label>
              )}
            </If>
            <If when={c.dataDirError()}>
              {(error) => (
                <Banner
                  tone="danger"
                  message={`${text("settings.data.error")}: ${props.store.errorText(error())}`}
                >
                  <Button
                    label={text("common.retry")}
                    variant="ghost"
                    size="sm"
                    onPress={() => void c.loadDataDirectory()}
                  />
                </Banner>
              )}
            </If>
          </Section>
        </View>
      </Scroll>
    </Page>
  );
}
