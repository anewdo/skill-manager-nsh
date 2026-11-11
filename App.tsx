import { Overlay, mount, signal } from "nsh";
import { Text, View } from "nsh";
import { Tab, Tabs } from "nsh/base";
import { Window } from "nsh/desktop";
import { primaryDisplayWorkArea } from "./lib/api";
import { Icon, type IconColor, type IconName } from "./components/icon";
import { Banner } from "./components/ui";
import { describeError } from "./lib/errors";
import { translate, type MessageKey } from "./i18n";
import { MarketplacePage } from "./pages/marketplace/Marketplace";
import { MySkillsPage } from "./pages/myskills/MySkills";
import { SettingsPage } from "./pages/settings/Settings";
import { createAppStore, type AppStore, type ToastTone } from "./store";
import type { AppError } from "./lib/types";

type Page = "marketplace" | "library" | "settings";

const toastTone: Record<
  ToastTone,
  { border: string; icon: IconName; iconColor: IconColor }
> = {
  success: { border: "border-primary/40", icon: "check", iconColor: "primary" },
  error: { border: "border-destructive/40", icon: "warning", iconColor: "destructive" },
  info: { border: "border-border", icon: "info", iconColor: "muted-foreground" },
};

function ToastHost(props: { store: AppStore }) {
  return (
    <Overlay
      style={{
        pointerEvents: "box-none",
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
        paddingHorizontal: 16,
        paddingTop: 16,
        alignItems: "flex-end",
      }}
    >
      <View class="w-[360px] gap-2" style={{ pointerEvents: "box-none" }}>
        <Each of={props.store.toasts()} key={(toast) => toast.id}>
          {(toast) => (
            <View
              class={`flex-row items-start gap-2 rounded-(--radius) border bg-card px-3 py-2 shadow ${toastTone[toast().tone].border}`}
              accessibilityLabel={toast().message}
              onPress={() => props.store.dismissToast(toast().id)}
            >
              <Icon
                name={toastTone[toast().tone].icon}
                size={16}
                color={toastTone[toast().tone].iconColor}
              />
              <Text class="flex-1 text-sm text-card-foreground">{toast().message}</Text>
            </View>
          )}
        </Each>
      </View>
    </Overlay>
  );
}

const navItemClass =
  "w-full flex-row items-center rounded-(--radius) px-3 py-2 active:bg-accent selected:bg-secondary";
const navTextClass = "text-sm text-muted-foreground group-selected:text-foreground";

function NavTab(props: {
  page: Page;
  current: () => Page;
  label: string;
  icon: "store" | "library" | "settings";
  onPress: () => void;
  testID: string;
}) {
  return (
    <Tab
      selected={props.current() === props.page}
      onPress={props.onPress}
      class={navItemClass}
      testID={props.testID}
    >
      <View class="flex-row items-center gap-2">
        <Icon
          name={props.icon}
          color={props.current() === props.page ? "foreground" : "muted-foreground"}
        />
        <Text class={navTextClass}>{props.label}</Text>
      </View>
    </Tab>
  );
}

const initialWindowWidth = 1200;
const initialWindowHeight = 720;

/** Sizes the window and centers it in the primary display's work area. */
function placeWindow(): void {
  const area = primaryDisplayWorkArea();
  if (!area) {
    return;
  }
  const width = Math.min(initialWindowWidth, area.width - 48);
  const height = Math.min(initialWindowHeight, area.height - 48);
  Window.setFrame({
    size: { width, height },
    origin: {
      x: Math.round(area.x + (area.width - width) / 2),
      y: Math.round(area.y + (area.height - height) / 2),
    },
  });
}

function App() {
  const store = createAppStore();
  store.start();
  placeWindow();
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
          <NavTab
            page="marketplace"
            current={page}
            label={text("nav.marketplace")}
            icon="store"
            onPress={() => page.set("marketplace")}
            testID="nav-marketplace"
          />
          <NavTab
            page="library"
            current={page}
            label={text("nav.library")}
            icon="library"
            onPress={() => page.set("library")}
            testID="nav-library"
          />
          <NavTab
            page="settings"
            current={page}
            label={text("nav.settings")}
            icon="settings"
            onPress={() => page.set("settings")}
            testID="nav-settings"
          />
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
        <If when={page() === "settings"}>{() => <SettingsPage store={store} />}</If>
      </View>
      <ToastHost store={store} />
    </View>
  );
}

mount(App);
