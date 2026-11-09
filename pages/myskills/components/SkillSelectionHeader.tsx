import { Text, View } from "nsh";
import { SelectionCheckbox } from "./SelectionCheckbox";
import type { TextFn } from "../labels";

export interface SkillSelectionHeaderProps {
  text: TextFn;
  selectedCount: () => number;
  allSelected: () => boolean;
  someSelected: () => boolean;
  disabled: () => boolean;
  onChange: (checked: boolean) => void;
}

export function SkillSelectionHeader(props: SkillSelectionHeaderProps) {
  return (
    <View class="flex-row items-center gap-2 px-1">
      <SelectionCheckbox
        checked={props.allSelected}
        indeterminate={props.someSelected}
        disabled={props.disabled}
        label={props.text("myskills.selectedCount", { count: props.selectedCount() })}
        onChange={props.onChange}
      />
      <Text class="text-xs text-muted-foreground">
        {props.text("myskills.selectedCount", { count: props.selectedCount() })}
      </Text>
    </View>
  );
}
