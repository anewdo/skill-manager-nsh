import { Overlay, View, type ViewStyle } from "nsh";
import {
  Toast,
  ToastStack,
  type Anchor,
  type ToastTransitionStatus,
} from "nsh/base";
import { Icon, type IconColor, type IconName } from "../icon";
import { cx } from "./cx";
import { Label } from "./text";

export type ToastTone = "success" | "error" | "warning" | "info";

/** The window edge the stack is anchored to; see `ToastStack`'s `placement`. */
export type ToastPlacement = Anchor;

export interface ToastMessage {
  id: string;
  tone: ToastTone;
  message: string;
  status: ToastTransitionStatus;
}

/** A narrower stack than the old 360px host. */
export const TOAST_WIDTH = 300;

/** Layers shown while the stack is collapsed, and the store's on-screen limit. */
export const MAX_TOASTS = 3;

const toastLook: Record<ToastTone, { border: string; icon: IconName; color: IconColor }> = {
  success: { border: "border-success/40", icon: "circleCheck", color: "success" },
  error: { border: "border-destructive/40", icon: "error", color: "destructive" },
  warning: { border: "border-warning/40", icon: "warning", color: "warning" },
  info: { border: "border-border", icon: "info", color: "muted-foreground" },
};

/** Where the Overlay puts the stack for each of the eight anchors. */
const alignment: Record<
  ToastPlacement,
  { cross: ViewStyle["alignItems"]; main: ViewStyle["justifyContent"] }
> = {
  topLeft: { cross: "flex-start", main: "flex-start" },
  topCenter: { cross: "center", main: "flex-start" },
  topRight: { cross: "flex-end", main: "flex-start" },
  bottomLeft: { cross: "flex-start", main: "flex-end" },
  bottomCenter: { cross: "center", main: "flex-end" },
  bottomRight: { cross: "flex-end", main: "flex-end" },
  leftCenter: { cross: "flex-start", main: "center" },
  rightCenter: { cross: "flex-end", main: "center" },
};

function overlayStyle(placement: ToastPlacement): ViewStyle {
  const { cross, main } = alignment[placement];
  return {
    pointerEvents: "box-none",
    left: 0,
    right: 0,
    top: 0,
    bottom: 0,
    paddingHorizontal: 16,
    paddingTop: 16,
    paddingBottom: 16,
    alignItems: cross,
    justifyContent: main,
  };
}

export interface ToastHostProps {
  toasts: ToastMessage[];
  onDismiss: (id: string) => void;
  /** The window edge to appear at (default: top right). */
  placement?: ToastPlacement;
  /** The stack was expanded or collapsed by the pointer. */
  onExpandedChange?: (expanded: boolean) => void;
  /** Escape or the back button while it holds toasts. */
  onCancel?: () => void;
}

/** The app's toasts: nsh/base's stack of layers at one window edge. */
export function ToastHost(props: ToastHostProps) {
  const placement = props.placement ?? "topRight";
  const upward = placement.startsWith("bottom");
  const leftward = placement.endsWith("Left") || placement === "leftCenter";

  return (
    <Overlay style={overlayStyle(placement)}>
      <ToastStack
        placement={placement}
        motion={{ collapsedVisible: MAX_TOASTS }}
        style={{ width: TOAST_WIDTH, maxWidth: "100%" }}
        closeOnEscape={props.onCancel !== undefined}
        onExpandedChange={(event) => props.onExpandedChange?.(event.expanded)}
        onCancel={props.onCancel}
      >
        <Each of={props.toasts} key={(toast) => toast.id}>
          {(toast) => (
            <Toast
              transitionStatus={toast().status}
              class={cx(
                "rounded-(--radius) border bg-popover px-3 py-2.5 shadow-lg transition-all duration-200 ease-out",
                toastLook[toast().tone].border,
              )}
              startingClass={upward ? "opacity-0 -translate-y-4" : "opacity-0 translate-y-4"}
              endingClass="opacity-0 scale-95"
              swipingClass="transition-none"
              swipeDirection={leftward ? "left" : "right"}
              accessibilityLabel={toast().message}
              onDismissRequest={() => props.onDismiss(toast().id)}
            >
              <View
                class="flex-row items-start gap-2"
                onPress={() => props.onDismiss(toast().id)}
                testID={`toast-${toast().tone}`}
              >
                <Icon name={toastLook[toast().tone].icon} color={toastLook[toast().tone].color} />
                <Label class="flex-1">{toast().message}</Label>
              </View>
            </Toast>
          )}
        </Each>
      </ToastStack>
    </Overlay>
  );
}
