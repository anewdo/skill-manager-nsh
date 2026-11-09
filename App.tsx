import { mount, signal } from "nsh";
import { Text, View } from "nsh";
import { Tab, Tabs } from "nsh/base";
import { Banner } from "./components/ui";
import { describeError } from "./errors";
import { translate, type MessageKey } from "./i18n";
import { MarketplacePage } from "./pages/marketplace/Marketplace";
import { MySkillsPage } from "./pages/myskills/MySkills";
import { createAppStore } from "./store";
import type { AppError } from "./types";

type Page = "marketplace" | "library";

const navItemClass =
  "w-full flex-row items-center rounded-(--radius) px-3 py-2 active:bg-accent selected:bg-secondary";
const navTextClass = "text-sm text-muted-foreground group-selected:text-foreground";

function App() {
  const store = createAppStore();
  store.start();
  const page = signal<Page>("marketplace");

  const text = (key: MessageKey): string =>
    translate(store.settings()?.language ?? "zh_cn")(key);

  const errorMessage = (error: AppError): string =>
    `${text("app.stateError")}: ${describeError(
      error,
      translate(store.settings()?.language ?? "zh_cn"),
    )}`;

  return (
    <View class="flex-1 flex-row bg-background">
      <View class="w-[208px] gap-4 border-r border-border bg-card px-3 py-4">
        <Text class="px-3 text-base font-semibold text-foreground">{text("app.title")}</Text>
        <Tabs class="flex-col gap-1">
          <Tab
            selected={page() === "marketplace"}
            onPress={() => page.set("marketplace")}
            class={navItemClass}
            testID="nav-marketplace"
          >
            <Text class={navTextClass}>{text("nav.marketplace")}</Text>
          </Tab>
          <Tab
            selected={page() === "library"}
            onPress={() => page.set("library")}
            class={navItemClass}
            testID="nav-library"
          >
            <Text class={navTextClass}>{text("nav.library")}</Text>
          </Tab>
        </Tabs>
      </View>
      <View class="flex-1">
        <View class="gap-2 px-6 pt-4">
          <If when={store.settingsError()}>
            {(error) => <Banner tone="warning" message={errorMessage(error())} />}
          </If>
          <If when={store.syncError()}>
            {(error) => <Banner tone="error" message={errorMessage(error())} />}
          </If>
        </View>
        <If when={page() === "marketplace"}>{() => <MarketplacePage store={store} />}</If>
        <If when={page() === "library"}>{() => <MySkillsPage store={store} />}</If>
      </View>
    </View>
  );
}

mount(App);
