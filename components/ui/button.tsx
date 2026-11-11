import { View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { Icon, type IconColor, type IconName } from "../icon";
import { cx } from "./cx";
import { Spinner } from "./feedback";
import { Label, type LabelTone } from "./text";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md";

const buttonBase =
  "flex-row items-center justify-center gap-1.5 rounded-(--radius) disabled:opacity-50";

const buttonVariantClass: Record<ButtonVariant, string> = {
  primary: "bg-primary hover:bg-primary/90 active:bg-primary/80",
  secondary: "border border-border bg-card hover:bg-accent active:bg-accent",
  ghost: "hover:bg-accent active:bg-accent",
  danger:
    "border border-destructive/40 bg-destructive/10 hover:bg-destructive/20 active:bg-destructive/20",
};

const buttonSizeClass: Record<ButtonSize, string> = {
  sm: "h-7 px-2.5",
  md: "h-8 px-3",
};

const buttonTone: Record<ButtonVariant, LabelTone> = {
  primary: "onPrimary",
  secondary: "default",
  ghost: "muted",
  danger: "danger",
};

const buttonIconColor: Record<ButtonVariant, IconColor> = {
  primary: "primary-foreground",
  secondary: "foreground",
  ghost: "muted-foreground",
  danger: "destructive",
};

export interface ButtonProps {
  label: string;
  onPress: () => void;
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: IconName;
  disabled?: boolean;
  /** Shows a rotating indicator in place of the icon and blocks presses. */
  busy?: boolean;
  testID?: string;
}

export function Button(props: ButtonProps) {
  return (
    <BaseButton
      class={cx(
        buttonBase,
        buttonVariantClass[props.variant ?? "secondary"],
        buttonSizeClass[props.size ?? "md"],
      )}
      disabled={props.disabled === true || props.busy === true}
      onPress={props.onPress}
      testID={props.testID}
    >
      <If when={props.busy === true}>
        {() => <Spinner size={14} color={buttonIconColor[props.variant ?? "secondary"]} />}
      </If>
      <If when={props.busy === true ? undefined : props.icon}>
        {(icon) => (
          <Icon name={icon()} size={14} color={buttonIconColor[props.variant ?? "secondary"]} />
        )}
      </If>
      <Label
        variant={props.size === "sm" ? "caption" : "body"}
        tone={buttonTone[props.variant ?? "secondary"]}
      >
        {props.label}
      </Label>
    </BaseButton>
  );
}

/** The bubble an icon-only control shows while the pointer rests on it. */
export function Tip(props: { text: string }): JSX.Element {
  return (
    <View class="rounded-md border border-border bg-popover px-2 py-1">
      <Label variant="caption">{props.text}</Label>
    </View>
  );
}

export type IconButtonTone = "default" | "danger";

const iconButtonClass: Record<IconButtonTone, string> = {
  default:
    "h-7 w-7 items-center justify-center rounded-(--radius) hover:bg-accent active:bg-accent disabled:opacity-40",
  danger:
    "h-7 w-7 items-center justify-center rounded-(--radius) hover:bg-destructive/10 active:bg-destructive/20 disabled:opacity-40",
};

export interface IconButtonProps {
  icon: IconName;
  /** What it does: read by screen readers and shown as its tooltip. */
  label: string;
  onPress: () => void;
  tone?: IconButtonTone;
  disabled?: boolean;
  testID?: string;
}

export function IconButton(props: IconButtonProps) {
  return (
    <BaseButton
      class={iconButtonClass[props.tone ?? "default"]}
      disabled={props.disabled === true}
      onPress={props.onPress}
      accessibilityLabel={props.label}
      tooltip={<Tip text={props.label} />}
      testID={props.testID}
    >
      <Icon
        name={props.icon}
        color={props.tone === "danger" ? "destructive" : "muted-foreground"}
      />
    </BaseButton>
  );
}
