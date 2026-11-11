import { View, type JSX } from "nsh";
import { Icon, type IconName } from "../icon";
import { cx } from "./cx";
import { Label } from "./text";

/** The root of a page: it fills the area beside the sidebar. */
export function Page(props: { children?: JSX.Element }) {
  return <View class="flex-1 bg-background">{props.children}</View>;
}

export interface PageHeaderProps {
  title: string;
  subtitle?: string;
  /** Its actions, at the end of the title's row. */
  children?: JSX.Element;
}

export function PageHeader(props: PageHeaderProps) {
  return (
    <View class="flex-row items-center gap-3 px-6 pt-5 pb-3">
      <View class="flex-1 gap-0.5">
        <Label variant="title">{props.title}</Label>
        <If when={props.subtitle}>
          {(subtitle) => (
            <Label variant="caption" tone="muted">
              {subtitle()}
            </Label>
          )}
        </If>
      </View>
      {props.children}
    </View>
  );
}

/** A row of filters and view options under a page's header. */
export function Toolbar(props: { children?: JSX.Element }) {
  return <View class="flex-row items-center gap-3 px-6 pb-3">{props.children}</View>;
}

/** The messages a page shows above its content; it takes no room while empty. */
export function Notices(props: { children?: JSX.Element }) {
  return <View class="gap-2 px-6">{props.children}</View>;
}

export interface CardProps {
  /** Layout classes only: padding, gaps, `flex-1`. */
  class?: string;
  testID?: string;
  children?: JSX.Element;
}

export function Card(props: CardProps) {
  return (
    <View
      class={cx("rounded-(--radius) border border-border bg-card", props.class)}
      testID={props.testID}
    >
      {props.children}
    </View>
  );
}

export interface ListRowProps {
  /** With it the row takes presses and shows it under the pointer. */
  onPress?: () => void;
  testID?: string;
  children?: JSX.Element;
}

/** One item of a list: a card laying its children out in a row. */
export function ListRow(props: ListRowProps) {
  return (
    <View
      class="flex-row items-center gap-3 rounded-(--radius) border border-border bg-card px-4 py-2.5 hover:border-input"
      onPress={props.onPress === undefined ? undefined : () => props.onPress?.()}
      testID={props.testID}
    >
      {props.children}
    </View>
  );
}

export interface SectionProps {
  icon: IconName;
  title: string;
  hint?: string;
  children?: JSX.Element;
}

/** A titled card grouping the settings of one subject. */
export function Section(props: SectionProps) {
  return (
    <Card class="gap-4 p-5">
      <View class="gap-1">
        <View class="flex-row items-center gap-2">
          <Icon name={props.icon} color="foreground" />
          <Label variant="strong">{props.title}</Label>
        </View>
        <If when={props.hint}>
          {(hint) => (
            <Label variant="caption" tone="muted">
              {hint()}
            </Label>
          )}
        </If>
      </View>
      {props.children}
    </Card>
  );
}

export interface FieldProps {
  label: string;
  /** Its label is above the content instead of before it. */
  stacked?: boolean;
  children?: JSX.Element;
}

/** A labelled value or control: a settings row, a line of a detail list. */
export function Field(props: FieldProps) {
  return (
    <View class={props.stacked === true ? "gap-1.5" : "flex-row items-center gap-3"}>
      <Label tone="muted" class={props.stacked === true ? undefined : "w-28"}>
        {props.label}
      </Label>
      {props.children}
    </View>
  );
}
