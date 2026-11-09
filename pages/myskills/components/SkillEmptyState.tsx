import { Text, View } from "nsh";
import type { TextFn } from "../labels";

export interface SkillEmptyStateProps {
  text: TextFn;
  hasInstallations: () => boolean;
  hasQuery: () => boolean;
}

export function SkillEmptyState(props: SkillEmptyStateProps) {
  return (
    <View class="flex-1 items-center justify-center gap-2 p-12">
      <Text class="text-base font-medium text-foreground">
        {props.hasInstallations()
          ? props.hasQuery()
            ? props.text("myskills.searchEmpty")
            : props.text("myskills.filteredEmpty")
          : props.text("myskills.empty")}
      </Text>
      <If when={!props.hasInstallations()}>
        {() => (
          <Text class="text-sm text-muted-foreground">{props.text("myskills.emptyHint")}</Text>
        )}
      </If>
    </View>
  );
}
