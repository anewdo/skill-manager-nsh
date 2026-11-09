import { Text, View } from "nsh";
import { Progress } from "nsh/base";
import { ActionButton } from "../../../components/ui";
import type { OperationSnapshot } from "../../../types";
import { progressPercent } from "../model";

export interface SkillProgressBarProps {
  snapshot: () => OperationSnapshot;
  phase: (snapshot: OperationSnapshot) => string;
  canCancel: (snapshot: OperationSnapshot) => boolean;
  cancelLabel: string;
  onCancel: (snapshot: OperationSnapshot) => void;
}

export function SkillProgressBar(props: SkillProgressBarProps) {
  return (
    <View class="flex-row items-center gap-3 rounded-(--radius) border border-border bg-card px-3 py-2">
      <Text class="text-sm text-foreground">{props.phase(props.snapshot())}</Text>
      <Progress
        class="flex-1"
        value={progressPercent(props.snapshot().progress)}
        indeterminate={progressPercent(props.snapshot().progress) === undefined}
        trackClass="h-1.5 rounded-full bg-muted"
        indicatorClass="h-1.5 rounded-full bg-primary"
      />
      <If when={props.canCancel(props.snapshot())}>
        {() => (
          <ActionButton
            label={props.cancelLabel}
            variant="ghost"
            onPress={() => props.onCancel(props.snapshot())}
          />
        )}
      </If>
    </View>
  );
}
