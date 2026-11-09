import { Text, View } from "nsh";
import { Badge } from "../../../components/ui";
import {
  localStateKeys,
  localStateTone,
  updateStateKeys,
  updateStateTone,
} from "../labels";
import { sourceLabel } from "../model";
import { SelectionCheckbox } from "./SelectionCheckbox";
import { SkillActions } from "./SkillActions";
import type { SkillViewProps } from "./SkillListItem";

export function SkillCard(props: SkillViewProps) {
  const text = props.text;
  return (
    <View class="flex-1 gap-2 rounded-(--radius) border border-border bg-card p-4">
      <View class="flex-row items-start gap-2">
        <SelectionCheckbox
          checked={props.selected}
          disabled={props.busy}
          label={props.installation().display_name}
          onChange={props.onToggle}
        />
        <View class="flex-1 gap-1">
          <Text class="text-sm font-semibold text-foreground">
            {props.installation().display_name}
          </Text>
          <View class="flex-row flex-wrap gap-1">
            <Badge
              label={text(localStateKeys[props.installation().local_state])}
              tone={localStateTone(props.installation().local_state)}
            />
            <Badge
              label={text(updateStateKeys[props.installation().update_state])}
              tone={updateStateTone(props.installation().update_state)}
            />
          </View>
        </View>
      </View>
      <Text class="text-xs text-muted-foreground">
        {sourceLabel(props.installation().source)}
      </Text>
      <Text class="text-xs text-muted-foreground">{props.installation().root_relpath}</Text>
      <If when={props.installation().last_error}>
        {(message) => <Text class="text-xs text-destructive">{message()}</Text>}
      </If>
      <View class="flex-1" />
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
