import { View } from "nsh";
import { Tab, Tabs } from "nsh/base";
import { Icon, type IconName } from "../components/icon";
import { Label } from "../components/ui";
import type { MessageKey } from "../i18n";
import type { AppStore } from "../store";
import { OperationsIndicator } from "./OperationsIndicator";

/** The pages the app has; the Agents page joins with the deployment engine. */
export type PageId = "library" | "marketplace" | "settings";

interface NavEntry {
  page: PageId;
  icon: IconName;
  label: MessageKey;
}

const navEntries: NavEntry[] = [
  { page: "library", icon: "library", label: "nav.library" },
  { page: "marketplace", icon: "store", label: "nav.marketplace" },
  { page: "settings", icon: "settings", label: "nav.settings" },
];

export interface SidebarProps {
  store: AppStore;
  current: PageId;
  onNavigate: (page: PageId) => void;
}

export function Sidebar(props: SidebarProps) {
  const text = props.store.text;
  return (
    <View class="w-[200px] gap-4 border-r border-sidebar-border bg-sidebar px-3 pt-5 pb-3">
      <Label variant="heading" class="px-3">
        {text("app.title")}
      </Label>
      <Tabs class="flex-1 flex-col gap-0.5">
        <Each of={navEntries}>
          {(entry) => (
            <Tab
              selected={props.current === entry().page}
              onPress={() => props.onNavigate(entry().page)}
              class="group w-full flex-row items-center gap-2.5 rounded-(--radius) px-3 py-2 hover:bg-sidebar-accent selected:bg-sidebar-accent"
              testID={`nav-${entry().page}`}
            >
              <Icon
                name={entry().icon}
                color={props.current === entry().page ? "primary" : "muted-foreground"}
              />
              <Label tone="muted" class="group-selected:text-foreground group-selected:font-medium">
                {text(entry().label)}
              </Label>
            </Tab>
          )}
        </Each>
      </Tabs>
      <OperationsIndicator store={props.store} />
    </View>
  );
}
