import { mount } from "nsh";
import { Text, View } from "nsh";
import { Banner } from "./components/ui";
import { describeError } from "./errors";
import { translate, type MessageKey } from "./i18n";
import { MarketplacePage } from "./pages/marketplace/Marketplace";
import { createAppStore } from "./store";

function App() {
  const store = createAppStore();
  store.start();

  const text = (key: MessageKey): string =>
    translate(store.settings()?.language ?? "zh_cn")(key);

  return (
    <View class="flex-1 bg-background">
      <View class="border-b border-border bg-card px-6 py-3">
        <Text class="text-lg font-semibold text-foreground">{text("app.title")}</Text>
      </View>
      <View class="gap-2 px-6 pt-4">
        <If when={store.settingsError()}>
          {(error) => (
            <Banner
              tone="warning"
              message={`${text("app.stateError")}: ${describeError(
                error(),
                translate(store.settings()?.language ?? "zh_cn"),
              )}`}
            />
          )}
        </If>
        <If when={store.syncError()}>
          {(error) => (
            <Banner
              tone="error"
              message={`${text("app.stateError")}: ${describeError(
                error(),
                translate(store.settings()?.language ?? "zh_cn"),
              )}`}
            />
          )}
        </If>
      </View>
      <MarketplacePage store={store} />
    </View>
  );
}

mount(App);
