import { Text, View } from "nsh";
import { Dialog } from "nsh/base";
import { ActionButton, Banner } from "../../../components/ui";
import type { Installation } from "../../../types";
import type { TextFn } from "../labels";

export interface SkillDeleteDialogProps {
  open: () => boolean;
  items: () => Installation[];
  modifiedCount: () => number;
  text: TextFn;
  onConfirm: () => void;
  onClose: () => void;
}

export function SkillDeleteDialog(props: SkillDeleteDialogProps) {
  const text = props.text;
  return (
    <Dialog
      open={props.open()}
      animation="fade"
      class="m-auto w-[520px] gap-4 rounded-(--radius) border border-border bg-popover p-6"
      backdropClass="bg-black/40"
      onOpenChange={(event) => {
        if (!event.open) {
          props.onClose();
        }
      }}
    >
      <View class="gap-3">
        <Text class="text-base font-semibold text-foreground">{text("myskills.delete.title")}</Text>
        <Text class="text-sm text-foreground">
          {props.items().length === 1
            ? text("myskills.delete.single", {
                name: props.items()[0]?.display_name ?? "",
              })
            : text("myskills.delete.batch", { count: props.items().length })}
        </Text>
        <If when={props.modifiedCount() > 0}>
          {() => (
            <Banner
              tone="warning"
              message={text("myskills.delete.modifiedWarning", {
                count: props.modifiedCount(),
              })}
            />
          )}
        </If>
        <View class="flex-row justify-end gap-2">
          <ActionButton label={text("common.cancel")} variant="ghost" onPress={props.onClose} />
          <ActionButton
            label={text("myskills.delete.confirm")}
            variant="danger"
            onPress={props.onConfirm}
          />
        </View>
      </View>
    </Dialog>
  );
}
