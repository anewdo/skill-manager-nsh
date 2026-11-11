import { Text, View, type JSX } from "nsh";
import { TextView } from "nsh/base";
import { cx } from "./cx";

/** The app's type scale. Pages pick a variant instead of writing sizes. */
export type LabelVariant = "title" | "heading" | "strong" | "body" | "caption";
export type LabelTone =
  | "default"
  | "muted"
  | "primary"
  | "danger"
  | "success"
  | "warning"
  | "info"
  | "onPrimary";

const variantClass: Record<LabelVariant, string> = {
  title: "text-lg font-semibold",
  heading: "text-base font-semibold",
  strong: "text-sm font-medium",
  body: "text-sm",
  caption: "text-xs",
};

const toneClass: Record<LabelTone, string> = {
  default: "text-foreground",
  muted: "text-muted-foreground",
  primary: "text-primary",
  danger: "text-destructive",
  success: "text-success",
  warning: "text-warning",
  info: "text-info",
  onPrimary: "text-primary-foreground",
};

const clampClass: Record<number, string> = {
  1: "truncate",
  2: "line-clamp-2",
  3: "line-clamp-3",
};

export interface LabelProps {
  variant?: LabelVariant;
  tone?: LabelTone;
  /** The most lines it shows before it is cut with an ellipsis (1 to 3). */
  lines?: 1 | 2 | 3;
  /** Layout classes only (`flex-1`, a width); color and size come from the props. */
  class?: string;
  /** Its text can be selected and copied. Read once, when the label is made. */
  selectable?: boolean;
  testID?: string;
  children?: JSX.Element;
}

function labelClass(props: LabelProps): string {
  return cx(
    variantClass[props.variant ?? "body"],
    toneClass[props.tone ?? "default"],
    props.lines !== undefined && clampClass[props.lines],
  );
}

export function Label(props: LabelProps) {
  // Selection belongs to views, so a selectable label is a view around its
  // text; whether it is one is decided once, when the label is made.
  if (props.selectable === true) {
    return (
      <View selectable class={props.class}>
        <Text class={labelClass(props)} testID={props.testID}>
          {props.children}
        </Text>
      </View>
    );
  }
  return (
    <Text class={cx(labelClass(props), props.class)} testID={props.testID}>
      {props.children}
    </Text>
  );
}

/** A Markdown document in the app's body text, selectable. */
export function Markdown(props: { source: string }) {
  return <TextView markdown={props.source} selectable class="text-sm text-foreground" />;
}
