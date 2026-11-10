import { Text, View } from "nsh";
import { Input, Tab, Tabs } from "nsh/base";
import { ActionButton, IconButton } from "../../../components/ui";
import { filterKeys, tabClass, tabTextClass, viewModeKeys, type TextFn } from "../labels";
import { statusFilters, viewModes, type StatusFilter, type ViewMode } from "../model";

export interface SkillToolbarProps {
  text: TextFn;
  query: () => string;
  filter: () => StatusFilter;
  counts: () => Record<StatusFilter, number>;
  total: () => number;
  view: () => ViewMode;
  selectedCount: () => number;
  busy: () => boolean;
  onQueryChange: (value: string) => void;
  onFilterChange: (filter: StatusFilter) => void;
  onViewChange: (view: ViewMode) => void;
  onCheckAll: () => void;
  onClearSelection: () => void;
  onDeleteSelected: () => void;
}

export function SkillToolbar(props: SkillToolbarProps) {
  const text = props.text;
  return (
    <View class="gap-3 border-b border-border bg-card px-6 py-4">
      <View class="flex-row items-center gap-3">
        <View class="flex-1 gap-1">
          <Text class="text-lg font-semibold text-foreground">{text("myskills.title")}</Text>
          <Text class="text-xs text-muted-foreground">
            {text("myskills.subtitle", { count: props.total() })}
          </Text>
        </View>
        <Input
          class="w-[260px] rounded-(--radius) border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
          value={props.query()}
          placeholder={text("myskills.searchPlaceholder")}
          submitBehavior="submit"
          onChange={(event) => props.onQueryChange(event.value)}
          testID="myskills-search"
        />
        <ActionButton
          label={text("myskills.checkAll")}
          variant="secondary"
          disabled={props.busy() || props.total() === 0}
          onPress={props.onCheckAll}
        />
      </View>
      <View class="flex-row items-center gap-3">
        <Tabs class="flex-row gap-1">
          <Each of={statusFilters}>
            {(item) => (
              <Tab
                selected={item() === props.filter()}
                onPress={() => props.onFilterChange(item())}
                class={tabClass}
              >
                <Text class={tabTextClass}>
                  {`${text(filterKeys[item()])} ${props.counts()[item()]}`}
                </Text>
              </Tab>
            )}
          </Each>
        </Tabs>
        <View class="flex-1" />
        <If when={props.selectedCount() > 0}>
          {() => (
            <View class="flex-row items-center gap-2">
              <Text class="text-xs text-muted-foreground">
                {text("myskills.selectedCount", { count: props.selectedCount() })}
              </Text>
              <ActionButton
                label={text("myskills.clearSelection")}
                variant="ghost"
                onPress={props.onClearSelection}
              />
              <IconButton
                icon="trash"
                label={text("myskills.deleteSelected", { count: props.selectedCount() })}
                tone="danger"
                count={props.selectedCount()}
                disabled={props.busy()}
                onPress={props.onDeleteSelected}
                testID="myskills-delete-selected"
              />
            </View>
          )}
        </If>
        <View class="flex-row items-center gap-1" accessibilityLabel={text("myskills.view.label")}>
          <Tabs class="flex-row gap-1">
            <Each of={viewModes}>
              {(mode) => (
                <Tab
                  selected={mode() === props.view()}
                  onPress={() => props.onViewChange(mode())}
                  class={tabClass}
                  testID={`myskills-view-${mode()}`}
                >
                  <Text class={tabTextClass}>{text(viewModeKeys[mode()])}</Text>
                </Tab>
              )}
            </Each>
          </Tabs>
        </View>
      </View>
    </View>
  );
}
