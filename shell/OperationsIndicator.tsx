import { computed, effect, signal, Scroll, View } from "nsh";
import { Button as BaseButton, Popover } from "nsh/base";
import { Icon } from "../components/icon";
import { Button, Label, Spinner } from "../components/ui";
import type { OperationSnapshot } from "../lib/types";
import type { AppStore } from "../store";
import { OperationRow } from "./OperationRow";

/**
 * The app-wide view of work in progress, at the foot of the sidebar: the
 * download queue (running, queued and failed installs) and the library
 * operation running now. It shows only while there is something to show.
 */
export function OperationsIndicator(props: { store: AppStore }) {
  const text = props.store.text;
  const open = signal(false);

  const rows = computed<OperationSnapshot[]>(() => {
    const queue = props.store.downloads.queue();
    const active = props.store.activeOperation();
    return active !== null && active.kind !== "install" ? [active, ...queue] : queue;
  });
  const running = computed(() => rows().some((row) => row.state !== "failed"));
  const hasFailed = computed(() => rows().some((row) => row.state === "failed"));

  effect(() => {
    if (rows().length === 0) {
      open.set(false);
    }
  });

  return (
    <If when={rows().length > 0}>
      {() => (
        <Popover
          open={open()}
          onOpenChange={(event) => open.set(event.open)}
          placement="top"
          align="start"
          offset={8}
          popupClass="w-[360px] gap-1 rounded-xl border border-border bg-popover p-2 shadow-lg"
          trigger={
            <BaseButton
              class="w-full flex-row items-center gap-2 rounded-(--radius) px-3 py-2 hover:bg-sidebar-accent active:bg-sidebar-accent"
              accessibilityLabel={text("operations.title")}
              testID="operations-indicator"
            >
              <If when={running()} else={<Icon name="warning" color="destructive" />}>
                {() => <Spinner />}
              </If>
              <Label class="flex-1">{text("operations.title")}</Label>
              <View class="rounded-full bg-sidebar-accent px-1.5 py-0.5">
                <Label variant="caption">{String(rows().length)}</Label>
              </View>
            </BaseButton>
          }
        >
          <View class="flex-row items-center justify-between px-2 py-1">
            <Label variant="strong">{text("operations.title")}</Label>
            <If when={hasFailed()}>
              {() => (
                <Button
                  label={text("operations.clearFailed")}
                  variant="ghost"
                  size="sm"
                  onPress={() => props.store.downloads.clearFailed()}
                />
              )}
            </If>
          </View>
          <Scroll class="max-h-[320px]">
            <Each of={rows()} key={(row) => row.id}>
              {(row) => <OperationRow store={props.store} snapshot={row()} />}
            </Each>
          </Scroll>
        </Popover>
      )}
    </If>
  );
}
