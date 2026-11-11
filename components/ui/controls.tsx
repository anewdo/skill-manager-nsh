import { View } from "nsh";
import {
  Checkbox as BaseCheckbox,
  Input,
  Switch as BaseSwitch,
  Tab,
  Tabs,
} from "nsh/base";
import { Icon, type IconName } from "../icon";
import { cx } from "./cx";
import { IconButton } from "./button";
import { Label } from "./text";

export interface TextFieldProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  onSubmit?: () => void;
  onBlur?: () => void;
  /** An icon before the text, as a search field has. */
  icon?: IconName;
  /** With it, a button that empties the field shows while it has text. */
  clearLabel?: string;
  /** Layout classes only: its width or `flex-1`. */
  class?: string;
  testID?: string;
}

export function TextField(props: TextFieldProps) {
  return (
    <View
      class={cx(
        "h-8 flex-row items-center gap-2 rounded-(--radius) border border-input bg-card pl-2.5 pr-1 focus-within:border-ring",
        props.class,
      )}
    >
      <If when={props.icon}>{(icon) => <Icon name={icon()} size={14} />}</If>
      <Input
        class="flex-1 text-sm text-foreground placeholder:text-muted-foreground"
        value={props.value}
        placeholder={props.placeholder}
        submitBehavior="submit"
        onChange={(event) => props.onChange(event.value)}
        onSubmit={() => props.onSubmit?.()}
        onBlur={() => props.onBlur?.()}
        testID={props.testID}
      />
      <If when={props.clearLabel !== undefined && props.value !== ""}>
        {() => (
          <IconButton
            icon="close"
            label={props.clearLabel ?? ""}
            onPress={() => props.onChange("")}
          />
        )}
      </If>
    </View>
  );
}

export interface SegmentedProps<T extends string> {
  options: readonly T[];
  value: T;
  /** The text of an option; read again when what it reads changes. */
  label: (option: T) => string;
  /** The user picked another option than `value`. */
  onChange: (option: T) => void;
  /** Each option's test id is this, a dash and the option. */
  testID?: string;
}

/** A row of exclusive options, one of them selected. */
export function Segmented<T extends string>(props: SegmentedProps<T>) {
  return (
    <Tabs class="flex-row gap-0.5 rounded-(--radius) bg-muted p-0.5">
      <Each of={props.options}>
        {(option) => (
          <Tab
            selected={option() === props.value}
            onPress={() => {
              if (option() !== props.value) {
                props.onChange(option());
              }
            }}
            class="group rounded-md px-2.5 py-1 hover:bg-accent selected:bg-card"
            testID={props.testID === undefined ? undefined : `${props.testID}-${option()}`}
          >
            <Label
              variant="caption"
              tone="muted"
              class="group-selected:text-foreground"
            >
              {props.label(option())}
            </Label>
          </Tab>
        )}
      </Each>
    </Tabs>
  );
}

export interface CheckboxProps {
  checked: boolean;
  /** Neither checked nor unchecked: a "select all" over a partial selection. */
  indeterminate?: boolean;
  disabled?: boolean;
  /** What it selects, for screen readers. */
  label: string;
  onChange: (checked: boolean) => void;
  testID?: string;
}

export function Checkbox(props: CheckboxProps) {
  return (
    <BaseCheckbox
      checked={props.checked}
      indeterminate={props.indeterminate === true}
      disabled={props.disabled === true}
      onChange={(event) => props.onChange(event.checked)}
      hitSlop={6}
      indicatorClass="h-4 w-4 items-center justify-center rounded border border-input bg-card checked:border-primary checked:bg-primary indeterminate:border-primary indeterminate:bg-primary disabled:opacity-50"
      indicator={
        <If when={props.checked || props.indeterminate === true}>
          {() => (
            <Icon
              name={props.checked ? "check" : "minus"}
              size={12}
              color="primary-foreground"
            />
          )}
        </If>
      }
      accessibilityLabel={props.label}
      testID={props.testID}
    />
  );
}

export interface SwitchProps {
  checked: boolean;
  disabled?: boolean;
  /** What it turns on, for screen readers. */
  label: string;
  onChange: (checked: boolean) => void;
  testID?: string;
}

export function Switch(props: SwitchProps) {
  return (
    <BaseSwitch
      checked={props.checked}
      disabled={props.disabled === true}
      onChange={(event) => props.onChange(event.checked)}
      hitSlop={6}
      trackClass="h-5 w-9 justify-center rounded-full bg-input px-0.5 checked:bg-primary disabled:opacity-50"
      thumbClass="h-4 w-4 rounded-full bg-card transition-transform duration-150 checked:translate-x-4"
      accessibilityLabel={props.label}
      testID={props.testID}
    />
  );
}
