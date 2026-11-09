import { Text, View } from "nsh";
import { Badge } from "../../../components/ui";
import type { Installation } from "../../../types";
import { localStateKeys, localStateTone, updateStateKeys, updateStateTone, type TextFn } from "../labels";
import { sourceLabel } from "../model";
import { SelectionCheckbox } from "./SelectionCheckbox";
import { SkillActions } from "./SkillActions";

export interface SkillViewProps {
  installation: () => Installation;
  selected: () => boolean;
  busy: () => boolean;
  text: TextFn;
  onToggle: (checked: boolean) => void;
  onDetail: () => void;
  onRename: () => void;
  onUpdate: () => void;
  onDelete: () => void;
}

export function SkillListItem(props: SkillViewProps) {
  const text = props.text;
  return (
    <View class="flex-row items-center gap-3 rounded-(--radius) border border-border bg-card px-4 py-3">
      <SelectionCheckbox
        checked={props.selected}
        disabled={props.busy}
        label={props.installation().display_name}
        onChange={props.onToggle}
      />
      <View class="flex-1 gap-1">
        <View class="flex-row flex-wrap items-center gap-2">
          <Text class="text-sm font-semibold text-foreground">
            {props.installation().display_name}
          </Text>
          <Badge
            label={text(localStateKeys[props.installation().local_state])}
            tone={localStateTone(props.installation().local_state)}
          />
          <Badge
            label={text(updateStateKeys[props.installation().update_state])}
            tone={updateStateTone(props.installation().update_state)}
          />
        </View>
        <Text class="text-xs text-muted-foreground">
          {`${sourceLabel(props.installation().source)} · ${props.installation().root_relpath}`}
        </Text>
        <If when={props.installation().last_error}>
          {(message) => <Text class="text-xs text-destructive">{message()}</Text>}
        </If>
      </View>
      <SkillActions
        installation={props.installation}
        busy={props.busy}
        text={text}
        onDetail={props.onDetail}
        onRename={props.onRename}
        onUpdate={props.onUpdate}
        onDelete={props.onDelete}
      />
    </View>
  );
}
