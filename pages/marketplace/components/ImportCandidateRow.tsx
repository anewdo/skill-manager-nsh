import { View } from "nsh";
import { Badge, Checkbox, Label, ListRow } from "../../../components/ui";
import type { ImportCandidate } from "../../../lib/types";
import type { TextFn } from "../../../store";

export interface ImportCandidateRowProps {
  candidate: ImportCandidate;
  selected: boolean;
  text: TextFn;
  /** Inside a group card: the row gives up its own card. */
  plain?: boolean;
  disabled?: boolean;
  onToggle: (checked: boolean) => void;
}

/** 本地发现的一项 Skill。 */
export function ImportCandidateRow(props: ImportCandidateRowProps) {
  return (
    <ListRow plain={props.plain}>
      <Checkbox
        checked={props.selected}
        disabled={props.disabled}
        label={props.candidate.name}
        onChange={props.onToggle}
        testID={`import-select-${props.candidate.name}`}
      />
      <View class="flex-1 gap-0.5">
        <View class="flex-row items-center gap-2">
          <Label variant="strong" lines={1} class="flex-1">
            {props.candidate.name}
          </Label>
          <Badge label={props.text("import.origin.custom")} />
        </View>
        <If when={props.candidate.description}>
          {(description) => (
            <Label variant="caption" tone="muted" lines={1}>
              {description()}
            </Label>
          )}
        </If>
        <Label variant="caption" tone="muted" lines={1}>
          {props.candidate.path}
        </Label>
      </View>
    </ListRow>
  );
}
