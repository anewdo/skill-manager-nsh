import { mount, signal, View, type JSX } from "nsh";
import { Scrollbar } from "nsh/base";
import { Window } from "nsh/desktop";
import { Banner, Notices, ToastHost } from "./components/ui";
import { primaryDisplayWorkArea } from "./lib/api";
import { LibraryPage } from "./pages/library/Library";
import { MarketplacePage } from "./pages/marketplace/Marketplace";
import { SettingsPage } from "./pages/settings/Settings";
import { Sidebar, type PageId } from "./shell/Sidebar";
import { createAppStore } from "./store";

/**
 * Keeps a page mounted once it has been opened and only hides it afterwards,
 * so switching back costs a style change instead of rebuilding the page.
 */
function PageSlot(props: {
  page: PageId;
  current: PageId;
  visited: ReadonlySet<PageId>;
  children: () => JSX.Element;
}) {
  return (
    <If when={props.visited.has(props.page)}>
      {() => (
        <View
          class="flex-1"
          style={{ display: props.current === props.page ? "flex" : "none" }}
          testID={`page-${props.page}`}
        >
          {props.children()}
        </View>
      )}
    </If>
  );
}

const initialWindowWidth = 1200;
const initialWindowHeight = 760;

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

/** Thin bars over the content's edge that show while it scrolls or is pointed at. */
function useAppScrollbars(): void {
  Scrollbar.setTheme({
    mode: "hover",
    motion: { enter: 150, exit: 400 },
    vertical: {
      class: "absolute inset-y-0 right-0 w-3 items-end p-0.5",
      thumbClass: "w-1.5 rounded-full bg-muted-foreground opacity-50",
    },
    horizontal: {
      class: "absolute inset-x-0 bottom-0 h-3 items-end p-0.5",
      thumbClass: "h-1.5 rounded-full bg-muted-foreground opacity-50",
    },
  });
}

function App() {
  const store = createAppStore();
  store.start();
  placeWindow();
  useAppScrollbars();

  const page = signal<PageId>("library");
  const visited = signal<ReadonlySet<PageId>>(new Set<PageId>(["library"]));
  const openPage = (next: PageId): void => {
    if (!visited().has(next)) {
      visited.set(new Set([...visited(), next]));
    }
    page.set(next);
  };

  return (
    <View class="flex-1 flex-row bg-background">
      <Sidebar store={store} current={page()} onNavigate={openPage} />
      <View class="flex-1">
        <If when={store.settingsError() ?? store.syncError()}>
          {() => (
            <View class="pt-4">
              <Notices>
                <If when={store.settingsError()}>
                  {(error) => (
                    <Banner
                      tone="warning"
                      message={`${store.text("app.stateError")}: ${store.errorText(error())}`}
                    />
                  )}
                </If>
                <If when={store.syncError()}>
                  {(error) => (
                    <Banner
                      tone="danger"
                      message={`${store.text("app.stateError")}: ${store.errorText(error())}`}
                    />
                  )}
                </If>
              </Notices>
            </View>
          )}
        </If>
        <PageSlot page="library" current={page()} visited={visited()}>
          {() => <LibraryPage store={store} onOpenMarketplace={() => openPage("marketplace")} />}
        </PageSlot>
        <PageSlot page="marketplace" current={page()} visited={visited()}>
          {() => <MarketplacePage store={store} />}
        </PageSlot>
        <PageSlot page="settings" current={page()} visited={visited()}>
          {() => <SettingsPage store={store} />}
        </PageSlot>
      </View>
      <ToastHost toasts={store.toasts()} onDismiss={store.dismissToast} />
    </View>
  );
}

mount(App);
