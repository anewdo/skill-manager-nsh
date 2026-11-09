import { View } from "nsh";
import trashIcon from "../../../assets/icons/trash.svg";
import { ActionButton, IconButton } from "../../../components/ui";
import type { Installation } from "../../../types";
import type { TextFn } from "../labels";
import { isUpdateAvailable } from "../model";

export interface SkillActionsProps {
  installation: () => Installation;
  busy: () => boolean;
  text: TextFn;
  onDetail: () => void;
  onRename: () => void;
  onUpdate: () => void;
  onDelete: () => void;
}

export function SkillActions(props: SkillActionsProps) {
  const text = props.text;
  return (
    <View class="flex-row items-center gap-1">
      <ActionButton
        label={text("myskills.details")}
        variant="ghost"
        onPress={props.onDetail}
      />
      <ActionButton
        label={text("myskills.rename")}
        variant="ghost"
        disabled={props.busy()}
        onPress={props.onRename}
      />
      <If when={isUpdateAvailable(props.installation())}>
        {() => (
          <ActionButton
            label={text("myskills.update")}
            variant="secondary"
            disabled={props.busy()}
            onPress={props.onUpdate}
          />
        )}
      </If>
      <IconButton
        icon={trashIcon}
        label={text("myskills.delete")}
        tone="danger"
        disabled={props.busy()}
        onPress={props.onDelete}
        testID={`myskills-delete-${props.installation().id}`}
      />
    </View>
  );
}
