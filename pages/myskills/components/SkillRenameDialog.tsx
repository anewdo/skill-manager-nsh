import { Text, View } from "nsh";
import { Dialog, Input } from "nsh/base";
import { ActionButton } from "../../../components/ui";
import type { Installation } from "../../../lib/types";
import type { TextFn } from "../labels";

export interface SkillRenameDialogProps {
  open: () => boolean;
  target: () => Installation | null;
  value: () => string;
  error: () => string | null;
  text: TextFn;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onClose: () => void;
}

export function SkillRenameDialog(props: SkillRenameDialogProps) {
  const text = props.text;
  return (
    <Dialog
      open={props.open()}
      animation="fade"
      class="m-auto w-[420px] gap-4 rounded-(--radius) border border-border bg-popover p-6"
      backdropClass="bg-black/40"
      onOpenChange={(event) => {
        if (!event.open) {
          props.onClose();
        }
      }}
    >
      <View class="gap-3">
        <Text class="text-base font-semibold text-foreground">{text("myskills.rename.title")}</Text>
        <Input
          class="rounded-(--radius) border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
          value={props.value()}
          placeholder={props.target()?.display_name ?? ""}
          submitBehavior="submit"
          onChange={(event) => props.onChange(event.value)}
          onSubmit={props.onSubmit}
          testID="myskills-rename-input"
        />
        <If when={props.error()}>
          {(message) => <Text class="text-xs text-destructive">{message()}</Text>}
        </If>
        <View class="flex-row justify-end gap-2">
          <ActionButton label={text("common.cancel")} variant="ghost" onPress={props.onClose} />
          <ActionButton
            label={text("myskills.rename.confirm")}
            variant="primary"
            onPress={props.onSubmit}
          />
        </View>
      </View>
    </Dialog>
  );
}
