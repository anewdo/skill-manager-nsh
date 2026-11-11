import { Text, View } from "nsh";
import { Dialog } from "nsh/base";
import { ActionButton } from "../../../components/ui";
import type { Installation } from "../../../lib/types";
import type { TextFn } from "../labels";

export interface SkillOverwriteDialogProps {
  open: () => boolean;
  target: () => Installation | null;
  text: TextFn;
  onConfirm: () => void;
  onClose: () => void;
}

export function SkillOverwriteDialog(props: SkillOverwriteDialogProps) {
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
        <Text class="text-base font-semibold text-foreground">
          {text("myskills.update.confirm.title")}
        </Text>
        <Text class="text-sm text-foreground">
          {text("myskills.update.confirm.message", {
            name: props.target()?.display_name ?? "",
          })}
        </Text>
        <View class="flex-row justify-end gap-2">
          <ActionButton label={text("common.cancel")} variant="ghost" onPress={props.onClose} />
          <ActionButton
            label={text("myskills.update.confirm.action")}
            variant="primary"
            onPress={props.onConfirm}
          />
        </View>
      </View>
    </Dialog>
  );
}
