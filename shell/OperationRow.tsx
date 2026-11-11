import { View } from "nsh";
import { IconButton, Label, ProgressBar, Spinner } from "../components/ui";
import { progressPercent } from "../lib/format";
import { isCancellable, operationDetail, operationTitle } from "../lib/operation-text";
import type { OperationSnapshot } from "../lib/types";
import type { AppStore } from "../store";

export interface OperationRowProps {
  store: AppStore;
  snapshot: OperationSnapshot;
}

/** One operation: what it is, where it is, and the way to cancel or dismiss it. */
export function OperationRow(props: OperationRowProps) {
  const text = props.store.text;
  return (
    <View class="flex-row items-center gap-2 px-2 py-1.5">
      <View class="flex-1 gap-1">
        <View class="flex-row items-baseline gap-1.5">
          <Label variant="strong" lines={1}>
            {operationTitle(text, props.snapshot)}
          </Label>
          <If when={props.snapshot.label?.source}>
            {(source) => (
              <Label variant="caption" tone="muted" lines={1} class="flex-1">
                {source()}
              </Label>
            )}
          </If>
        </View>
        <Label
          variant="caption"
          tone={props.snapshot.state === "failed" ? "danger" : "muted"}
          lines={2}
        >
          {operationDetail(
            text,
            props.snapshot,
            props.store.downloads.speeds()[props.snapshot.id],
          )}
        </Label>
        <If when={props.snapshot.state === "running"}>
          {() => <ProgressBar value={progressPercent(props.snapshot.progress)} />}
        </If>
      </View>
      <If when={props.snapshot.state === "failed"}>
        {() => (
          <IconButton
            icon="close"
            label={text("operation.dismiss")}
            onPress={() => props.store.downloads.dismiss(props.snapshot.id)}
          />
        )}
      </If>
      <If when={isCancellable(props.snapshot)}>
        {() => (
          <IconButton
            icon="close"
            label={text("operation.cancel")}
            onPress={() => props.store.cancelOperation(props.snapshot.id)}
            testID="operation-cancel"
          />
        )}
      </If>
      <If when={props.snapshot.state !== "failed" && props.snapshot.cancel_requested}>
        {() => <Spinner />}
      </If>
    </View>
  );
}
