import { Overlay, View } from "nsh";
import { Icon, type IconColor, type IconName } from "../icon";
import { cx } from "./cx";
import { Label } from "./text";

export type ToastTone = "success" | "error" | "warning" | "info";

export interface ToastMessage {
  id: number;
  tone: ToastTone;
  message: string;
}

const toastLook: Record<ToastTone, { border: string; icon: IconName; color: IconColor }> = {
  success: { border: "border-success/40", icon: "circleCheck", color: "success" },
  error: { border: "border-destructive/40", icon: "error", color: "destructive" },
  warning: { border: "border-warning/40", icon: "warning", color: "warning" },
  info: { border: "border-border", icon: "info", color: "muted-foreground" },
};

export interface ToastHostProps {
  toasts: ToastMessage[];
  onDismiss: (id: number) => void;
}

/** The app's toasts, stacked in the window's top right corner; a press dismisses one. */
export function ToastHost(props: ToastHostProps) {
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
        <Each of={props.toasts} key={(toast) => toast.id}>
          {(toast) => (
            <View
              class={cx(
                "flex-row items-start gap-2 rounded-(--radius) border bg-popover px-3 py-2.5 shadow-lg",
                toastLook[toast().tone].border,
              )}
              accessibilityLabel={toast().message}
              onPress={() => props.onDismiss(toast().id)}
              testID={`toast-${toast().tone}`}
            >
              <Icon name={toastLook[toast().tone].icon} color={toastLook[toast().tone].color} />
              <Label class="flex-1">{toast().message}</Label>
            </View>
          )}
        </Each>
      </View>
    </Overlay>
  );
}
