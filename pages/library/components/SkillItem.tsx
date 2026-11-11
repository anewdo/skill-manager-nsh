import { View } from "nsh";
import { Badge, Button, Card, Checkbox, IconButton, Label, ListRow } from "../../../components/ui";
import type { InstalledSkill } from "../../../lib/types";
import type { TextFn } from "../../../store";
import { localStateKeys, localStateTone, updateStateKeys, updateStateTone } from "../labels";
import { isUpdateAvailable, sourceLabel } from "../model";

export interface SkillItemProps {
  skill: InstalledSkill;
  selected: boolean;
  /** A library operation runs: the actions that start another are off. */
  busy: boolean;
  text: TextFn;
  onToggle: (skill: InstalledSkill, checked: boolean) => void;
  onDetail: (skill: InstalledSkill) => void;
  onRename: (skill: InstalledSkill) => void;
  onUpdate: (skill: InstalledSkill) => void;
  onDelete: (skill: InstalledSkill) => void;
}

/** The states worth a badge: what is wrong locally, and what the last update check found. */
function SkillBadges(props: { skill: InstalledSkill; text: TextFn }) {
  return (
    <View class="flex-row items-center gap-1">
      <If when={props.skill.local_state !== "clean" && props.skill.local_state !== "unknown"}>
        {() => (
          <Badge
            label={props.text(localStateKeys[props.skill.local_state])}
            tone={localStateTone[props.skill.local_state]}
          />
        )}
      </If>
      <If
        when={props.skill.update_state === "available" || props.skill.update_state === "check_failed"}
      >
        {() => (
          <Badge
            label={props.text(updateStateKeys[props.skill.update_state])}
            tone={updateStateTone[props.skill.update_state]}
          />
        )}
      </If>
    </View>
  );
}

function SkillActions(props: SkillItemProps) {
  return (
    <View class="flex-row items-center gap-1">
      <If when={isUpdateAvailable(props.skill)}>
        {() => (
          <Button
            label={props.text("library.update")}
            icon="update"
            size="sm"
            disabled={props.busy}
            onPress={() => props.onUpdate(props.skill)}
            testID={`library-update-${props.skill.display_name}`}
          />
        )}
      </If>
      <IconButton
        icon="rename"
        label={props.text("library.rename")}
        disabled={props.busy}
        onPress={() => props.onRename(props.skill)}
        testID={`library-rename-${props.skill.display_name}`}
      />
      <IconButton
        icon="trash"
        label={props.text("library.delete")}
        tone="danger"
        disabled={props.busy}
        onPress={() => props.onDelete(props.skill)}
        testID={`library-delete-${props.skill.display_name}`}
      />
    </View>
  );
}

function origin(skill: InstalledSkill, text: TextFn): string {
  return skill.source === null ? text("library.source.local") : sourceLabel(skill.source);
}

/** A skill as one line of the list view. Pressing its texts opens the detail. */
export function SkillListRow(props: SkillItemProps) {
  return (
    <ListRow testID={`library-skill-${props.skill.display_name}`}>
      <Checkbox
        checked={props.selected}
        disabled={props.busy}
        label={props.skill.display_name}
        onChange={(checked) => props.onToggle(props.skill, checked)}
      />
      <View
        class="flex-1 gap-0.5"
        onPress={() => props.onDetail(props.skill)}
        testID={`library-open-${props.skill.display_name}`}
      >
        <View class="flex-row items-center gap-2">
          <Label variant="strong" lines={1}>
            {props.skill.display_name}
          </Label>
          <SkillBadges skill={props.skill} text={props.text} />
        </View>
        <Label variant="caption" tone="muted" lines={1}>
          {origin(props.skill, props.text)}
        </Label>
        <If when={props.skill.last_error}>
          {(message) => (
            <Label variant="caption" tone="danger" lines={2}>
              {message()}
            </Label>
          )}
        </If>
      </View>
      <SkillActions {...props} />
    </ListRow>
  );
}

/** A skill as a card of the card view. Pressing its texts opens the detail. */
export function SkillCard(props: SkillItemProps) {
  return (
    <Card class="flex-1 gap-3 p-4" testID={`library-skill-${props.skill.display_name}`}>
      <View class="flex-row items-start gap-3">
        <Checkbox
          checked={props.selected}
          disabled={props.busy}
          label={props.skill.display_name}
          onChange={(checked) => props.onToggle(props.skill, checked)}
        />
        <View
          class="flex-1 gap-1"
          onPress={() => props.onDetail(props.skill)}
          testID={`library-open-${props.skill.display_name}`}
        >
          <Label variant="strong" lines={1}>
            {props.skill.display_name}
          </Label>
          <Label variant="caption" tone="muted" lines={1}>
            {origin(props.skill, props.text)}
          </Label>
          <If when={props.skill.last_error}>
            {(message) => (
              <Label variant="caption" tone="danger" lines={2}>
                {message()}
              </Label>
            )}
          </If>
        </View>
      </View>
      <View class="flex-row items-center justify-between gap-2">
        <SkillBadges skill={props.skill} text={props.text} />
        <SkillActions {...props} />
      </View>
    </Card>
  );
}
