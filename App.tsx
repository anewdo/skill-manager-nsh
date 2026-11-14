import { mount, signal, View, type JSX } from "nsh";
import { Scrollbar } from "nsh/base";
import { Application, displays, Window } from "nsh/desktop";
import { shutdownLocalImport } from "./lib/api";
import { Banner, Notices, ToastHost, type ToastPlacement } from "./components/ui";
import { primaryWorkArea, startingWindowBounds } from "./lib/window";
import { LibraryPage } from "./pages/library/Library";
import { MarketplacePage } from "./pages/marketplace/Marketplace";
import { SettingsPage } from "./pages/settings/Settings";
import { Sidebar, type PageId } from "./shell/Sidebar";
import { createAppStore } from "./store";

/** Any of ToastStack's anchors: topRight, bottomRight, bottomCenter, topCenter, ... */
const TOAST_PLACEMENT: ToastPlacement = "bottomRight";

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

/** Sizes the window and centers it in the primary display's work area. */
function placeWindow(): void {
  const bounds = startingWindowBounds(primaryWorkArea(displays()));
  if (!bounds) {
    return;
  }
  Window.get("main")?.setBounds(bounds);
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
  Application.on("quit", () => shutdownLocalImport());
  placeWindow();
  useAppScrollbars();

  const page = signal<PageId>("marketplace");
  const libraryFocusId = signal<string | null>(null);
  const visited = signal<ReadonlySet<PageId>>(new Set<PageId>(["marketplace"]));
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
          {() => <LibraryPage store={store} onOpenMarketplace={() => openPage("marketplace")}
            focusSkillId={libraryFocusId()} onFocusHandled={() => libraryFocusId.set(null)} />}
        </PageSlot>
        <PageSlot page="marketplace" current={page()} visited={visited()}>
          {() => <MarketplacePage store={store} onOpenLibrary={(id) => { libraryFocusId.set(id); openPage("library"); }} />}
        </PageSlot>
        <PageSlot page="settings" current={page()} visited={visited()}>
          {() => <SettingsPage store={store} />}
        </PageSlot>
      </View>
      <ToastHost
        toasts={store.toasts()}
        onDismiss={store.dismissToast}
        placement={TOAST_PLACEMENT}
        onExpandedChange={store.setToastsExpanded}
        onCancel={store.dismissNewestToast}
      />
    </View>
  );
}

mount(App);
