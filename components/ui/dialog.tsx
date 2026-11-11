import { View, type JSX } from "nsh";
import { Dialog } from "nsh/base";
import { cx } from "./cx";
import { IconButton } from "./button";
import { Label } from "./text";

export type ModalSize = "sm" | "md" | "lg";

const modalSizeClass: Record<ModalSize, string> = {
  sm: "w-[420px]",
  md: "w-[520px]",
  lg: "w-[720px] max-h-[680px]",
};

export interface ModalProps {
  open: boolean;
  title: string;
  subtitle?: string;
  size?: ModalSize;
  /** The label of the close button in its corner. */
  closeLabel: string;
  /** The user asked to close it: set `open` to false. */
  onClose: () => void;
  /** It is gone, after its exit: drop what it showed. */
  onClosed?: () => void;
  /** Its buttons, right-aligned under the content. */
  footer: JSX.Element;
  children?: JSX.Element;
}

export function Modal(props: ModalProps) {
  return (
    <Dialog
      open={props.open}
      animation="fade"
      class={cx(
        "m-auto rounded-xl border border-border bg-popover p-5",
        modalSizeClass[props.size ?? "md"],
      )}
      backdropClass="bg-overlay"
      onClose={() => props.onClosed?.()}
      onOpenChange={(event) => {
        if (!event.open) {
          props.onClose();
        }
      }}
    >
      <View class="gap-4">
        <View class="flex-row items-start gap-3">
          <View class="flex-1 gap-0.5">
            <Label variant="heading">{props.title}</Label>
            <If when={props.subtitle}>
              {(subtitle) => (
                <Label variant="caption" tone="muted" lines={1}>
                  {subtitle()}
                </Label>
              )}
            </If>
          </View>
          <IconButton
            icon="close"
            label={props.closeLabel}
            onPress={props.onClose}
            testID="modal-close"
          />
        </View>
        {props.children}
        <View class="flex-row items-center justify-end gap-2">{props.footer}</View>
      </View>
    </Dialog>
  );
}
