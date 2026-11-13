import { View } from "nsh";
import { Badge, Checkbox, Label, ListRow } from "../../../components/ui";
import type { ImportCandidate } from "../../../lib/types";
import type { TextFn } from "../../../store";
import { candidateOriginLabel } from "../import-model";

export interface ImportCandidateRowProps {
  candidate: ImportCandidate;
  selected: boolean;
  text: TextFn;
  /** Inside a group card: the row gives up its own card. */
  plain?: boolean;
  onToggle: (checked: boolean) => void;
}

/** One discovered skill, shared by the local and URL import tabs. */
export function ImportCandidateRow(props: ImportCandidateRowProps) {
  return (
    <ListRow plain={props.plain}>
      <Checkbox
        checked={props.selected}
        label={props.candidate.name}
        onChange={props.onToggle}
        testID={`import-select-${props.candidate.name}`}
      />
      <View class="flex-1 gap-0.5">
        <View class="flex-row items-center gap-2">
          <Label variant="strong" lines={1} class="flex-1">
            {props.candidate.name}
          </Label>
          <Badge label={candidateOriginLabel(props.candidate.origin, props.text)} />
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
