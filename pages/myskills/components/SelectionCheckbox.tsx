import { Text } from "nsh";
import { Checkbox } from "nsh/base";
import { checkboxIndicatorClass } from "../labels";

export interface SelectionCheckboxProps {
  checked: () => boolean;
  indeterminate?: () => boolean;
  disabled: () => boolean;
  label: string;
  onChange: (checked: boolean) => void;
}

export function SelectionCheckbox(props: SelectionCheckboxProps) {
  return (
    <Checkbox
      checked={props.checked()}
      indeterminate={props.indeterminate?.() === true}
      disabled={props.disabled()}
      onChange={(event) => props.onChange(event.checked)}
      indicatorClass={checkboxIndicatorClass}
      indicator={
        <Text class="text-[10px] text-primary-foreground">
          {props.checked() ? "✓" : props.indeterminate?.() === true ? "–" : ""}
        </Text>
      }
      accessibilityLabel={props.label}
    />
  );
}
