import { View, token, type JSX } from "nsh";
import { Progress } from "nsh/base";
import { Icon, type IconColor, type IconName } from "../icon";
import { cx } from "./cx";
import { Label, type LabelTone } from "./text";

export type Tone = "neutral" | "info" | "success" | "warning" | "danger";

export interface SpinnerProps {
  /** Its diameter in points, default 14. */
  size?: number;
  /** The theme token tinting it, default `muted-foreground`. */
  color?: IconColor;
}

/** A continuously rotating arc for work whose extent is unknown. */
export function Spinner(props: SpinnerProps) {
  return (
    <View
      class="animate-spin motion-reduce:animate-none"
      style={{
        width: props.size ?? 14,
        height: props.size ?? 14,
        borderRadius: (props.size ?? 14) / 2,
        borderWidth: Math.max(1, (props.size ?? 14) / 8),
        borderColor: token(props.color ?? "muted-foreground"),
        borderTopColor: "transparent",
        transformOrigin: "center",
        flexShrink: 0,
      }}
    />
  );
}

const toneIcon: Record<Tone, IconName> = {
  neutral: "info",
  info: "info",
  success: "circleCheck",
  warning: "warning",
  danger: "error",
};

const toneIconColor: Record<Tone, IconColor> = {
  neutral: "muted-foreground",
  info: "info",
  success: "success",
  warning: "warning",
  danger: "destructive",
};

const toneLabel: Record<Tone, LabelTone> = {
  neutral: "muted",
  info: "info",
  success: "success",
  warning: "warning",
  danger: "danger",
};

const bannerClass: Record<Tone, string> = {
  neutral: "border-border bg-muted",
  info: "border-info/30 bg-info/10",
  success: "border-success/30 bg-success/10",
  warning: "border-warning/30 bg-warning/10",
  danger: "border-destructive/30 bg-destructive/10",
};

export interface BannerProps {
  tone: Tone;
  message: string;
  testID?: string;
  /** Its action, a small button, after the message. */
  children?: JSX.Element;
}

/** A message that stays in the page until its cause is gone. */
export function Banner(props: BannerProps) {
  return (
    <View
      class={cx(
        "flex-row items-center gap-2 rounded-(--radius) border px-3 py-2",
        bannerClass[props.tone],
      )}
      testID={props.testID}
    >
      <Icon name={toneIcon[props.tone]} color={toneIconColor[props.tone]} />
      <Label class="flex-1">{props.message}</Label>
      {props.children}
    </View>
  );
}

const badgeClass: Record<Tone, string> = {
  neutral: "bg-secondary",
  info: "bg-info/15",
  success: "bg-success/15",
  warning: "bg-warning/15",
  danger: "bg-destructive/15",
};

export function Badge(props: { label: string; tone?: Tone }) {
  return (
    <View class={cx("rounded-full px-2 py-0.5", badgeClass[props.tone ?? "neutral"])}>
      <Label variant="caption" tone={toneLabel[props.tone ?? "neutral"]}>
        {props.label}
      </Label>
    </View>
  );
}

export interface EmptyStateProps {
  icon: IconName;
  title: string;
  hint?: string;
  /** Its action, below the texts. */
  children?: JSX.Element;
}

/** What a list shows in place of its rows while it has none. */
export function EmptyState(props: EmptyStateProps) {
  return (
    <View class="flex-1 items-center justify-center gap-3 p-12">
      <View class="h-12 w-12 items-center justify-center rounded-full bg-muted">
        <Icon name={props.icon} size={22} />
      </View>
      <View class="items-center gap-1">
        <Label variant="heading">{props.title}</Label>
        <If when={props.hint}>{(hint) => <Label tone="muted">{hint()}</Label>}</If>
      </View>
      {props.children}
    </View>
  );
}

/** A spinner and a line of text filling the space a list will take. */
export function LoadingState(props: { label: string }) {
  return (
    <View class="flex-1 flex-row items-center justify-center gap-2 p-12">
      <Spinner />
      <Label tone="muted">{props.label}</Label>
    </View>
  );
}

export interface ProgressBarProps {
  /** How far the work is, from 0 to 100; left out while its extent is unknown. */
  value?: number;
  class?: string;
}

export function ProgressBar(props: ProgressBarProps) {
  return (
    <Progress
      class={props.class}
      value={props.value}
      indeterminate={props.value === undefined}
      trackClass="h-1 rounded-full bg-muted"
      indicatorClass={
        props.value === undefined
          ? "h-1 w-1/3 rounded-full bg-primary animate-pulse"
          : "h-1 rounded-full bg-primary"
      }
    />
  );
}
