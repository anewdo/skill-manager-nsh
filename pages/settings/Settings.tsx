import { Scroll, View } from "nsh";
import { Icon } from "../../components/icon";
import {
  Banner,
  Button,
  IconButton,
  Label,
  Page,
  PageHeader,
  Section,
  Segmented,
  SettingRow,
  Switch,
  TextField,
} from "../../components/ui";
import type { MessageKey } from "../../i18n";
import type { ConnectivityTarget, Language, ProxyMode, Theme } from "../../lib/types";
import type { AppStore } from "../../store";
import { createSettingsController } from "./controller";

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

export function SettingsPage(props: { store: AppStore }) {
  const text = props.store.text;
  const c = createSettingsController(props.store);

  return (
    <Page>
      <PageHeader title={text("settings.title")} />

      <Scroll class="flex-1">
        <View class="flex-row justify-center">
          <View class="flex-1 max-w-[720px] gap-4 px-6 pb-6">
            <If when={c.saveError()}>
              {(message) => (
                <Banner tone="danger" message={message()} testID="settings-save-error">
                  <IconButton
                    icon="close"
                    label={text("settings.dismiss")}
                    onPress={c.dismissSaveError}
                  />
                </Banner>
              )}
            </If>

            <Section icon="palette" title={text("settings.appearance.title")}>
              <SettingRow label={text("settings.language")}>
                <Segmented
                  options={languages}
                  value={c.language()}
                  label={(option) => text(languageKeys[option])}
                  onChange={(option) => void c.setLanguage(option)}
                  testID="settings-language"
                />
              </SettingRow>
              <SettingRow label={text("settings.theme")}>
                <Segmented
                  options={themes}
                  value={c.theme()}
                  label={(option) => text(themeKeys[option])}
                  onChange={(option) => void c.setTheme(option)}
                  testID="settings-theme"
                />
              </SettingRow>
              <SettingRow label={text("settings.showMarketAvatars")}>
                <Switch
                  checked={c.showMarketAvatars()}
                  label={text("settings.showMarketAvatars")}
                  onChange={(checked) => void c.setShowMarketAvatars(checked)}
                  testID="settings-show-avatars"
                />
              </SettingRow>
            </Section>

            <Section icon="globe" title={text("settings.proxy.title")}>
              <SettingRow label={text("settings.proxy.mode")}>
                <Segmented
                  options={proxyModes}
                  value={c.proxyMode()}
                  label={(option) => text(proxyModeKeys[option])}
                  onChange={c.setProxyMode}
                  testID="settings-proxy"
                />
              </SettingRow>
              <If when={c.proxyMode() === "custom"}>
                {() => (
                  <SettingRow label={text("settings.proxy.url")}>
                    <View class="w-[320px] gap-1">
                      <TextField
                        class="w-full"
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
                  </SettingRow>
                )}
              </If>
              <SettingRow
                label={text("settings.proxy.connectivity")}
                end={
                  <View class="max-w-[260px] items-end gap-1">
                    <Button
                      label={text("settings.proxy.test")}
                      icon="globe"
                      busy={c.testBusy()}
                      onPress={() => void c.testConnectivity()}
                      testID="settings-test-connectivity"
                    />
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
                  </View>
                }
              >
                <View class="flex-1 flex-row items-center gap-3">
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
              </SettingRow>
            </Section>

            <Section icon="folder" title={text("settings.data.title")}>
              <SettingRow label={text("settings.data.directory")} stacked>
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
              </SettingRow>
            </Section>
          </View>
        </View>
      </Scroll>
    </Page>
  );
}
