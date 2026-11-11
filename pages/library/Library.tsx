import { List, View, windowSize } from "nsh";
import {
  Banner,
  Button,
  Checkbox,
  EmptyState,
  IconButton,
  Label,
  Notices,
  Page,
  PageHeader,
  Segmented,
  TextField,
  Toolbar,
} from "../../components/ui";
import type { InstalledSkill } from "../../lib/types";
import type { AppStore } from "../../store";
import { LibraryDialogs } from "./components/LibraryDialogs";
import { SkillDetailModal } from "./components/SkillDetailModal";
import { SkillCard, SkillListRow, type SkillItemProps } from "./components/SkillItem";
import { createLibraryController } from "./controller";
import { filterKeys, viewModeKeys } from "./labels";
import { cardColumnsFor, statusFilters, viewModes } from "./model";

export interface LibraryPageProps {
  store: AppStore;
  onOpenMarketplace: () => void;
}

export function LibraryPage(props: LibraryPageProps) {
  const text = props.store.text;
  const c = createLibraryController(props.store);

  /** What every item gets besides its skill and whether it is selected. */
  const itemProps: Omit<SkillItemProps, "skill" | "selected" | "busy"> = {
    text,
    onToggle: (skill, checked) => c.toggle(skill.id, checked),
    onDetail: c.openDetail,
    onRename: c.openRename,
    onUpdate: c.requestUpdate,
    onDelete: (skill) => c.requestDelete([skill.id]),
  };

  return (
    <Page>
      <PageHeader
        title={text("library.title")}
        subtitle={text("library.subtitle", { count: c.skills().length })}
      >
        <TextField
          class="w-[260px]"
          icon="search"
          value={c.query()}
          placeholder={text("library.searchPlaceholder")}
          clearLabel={text("common.clear")}
          onChange={(value) => c.query.set(value)}
          testID="library-search"
        />
        <Button
          label={text("library.checkAll")}
          icon="refresh"
          disabled={c.busy() || c.skills().length === 0}
          onPress={c.checkAll}
          testID="library-check-all"
        />
      </PageHeader>

      <Toolbar>
        <Checkbox
          checked={c.allFilteredSelected()}
          indeterminate={c.someFilteredSelected()}
          disabled={c.busy() || c.filtered().length === 0}
          label={text("library.selectAll")}
          onChange={c.toggleAll}
          testID="library-select-all"
        />
        <If
          when={c.selected().length > 0}
          else={
            <Segmented
              options={statusFilters}
              value={c.filter()}
              label={(option) => `${text(filterKeys[option])} ${c.counts()[option]}`}
              onChange={(value) => c.filter.set(value)}
              testID="library-filter"
            />
          }
        >
          {() => (
            <View class="flex-row items-center gap-1">
              <Label tone="muted">
                {text("library.selectedCount", { count: c.selected().length })}
              </Label>
              <IconButton
                icon="close"
                label={text("library.clearSelection")}
                onPress={c.clearSelection}
              />
              <Button
                label={text("library.deleteSelected", { count: c.selected().length })}
                icon="trash"
                variant="danger"
                size="sm"
                disabled={c.busy()}
                onPress={() => c.requestDelete(c.selected().map((skill) => skill.id))}
                testID="library-delete-selected"
              />
            </View>
          )}
        </If>
        <View class="flex-1" />
        <Segmented
          options={viewModes}
          value={c.view()}
          label={(option) => text(viewModeKeys[option])}
          onChange={(value) => c.view.set(value)}
          testID="library-view"
        />
      </Toolbar>

      <If when={props.store.skillsError()}>
        {(error) => (
          <View class="pb-3">
            <Notices>
              <Banner tone="danger" message={props.store.errorText(error())}>
                <Button
                  label={text("common.retry")}
                  variant="ghost"
                  size="sm"
                  onPress={() => void props.store.refreshSkills()}
                />
              </Banner>
            </Notices>
          </View>
        )}
      </If>

      <If when={c.skills().length === 0 && props.store.skillsError() === null}>
        {() => (
          <EmptyState
            icon="package"
            title={text("library.empty")}
            hint={text("library.emptyHint")}
          >
            <Button
              label={text("library.openMarketplace")}
              icon="store"
              variant="primary"
              onPress={props.onOpenMarketplace}
              testID="library-open-marketplace"
            />
          </EmptyState>
        )}
      </If>

      <If when={c.skills().length > 0 && c.filtered().length === 0}>
        {() => (
          <EmptyState
            icon="searchX"
            title={
              c.query().trim() !== "" ? text("library.searchEmpty") : text("library.filteredEmpty")
            }
          />
        )}
      </If>

      <If when={c.filtered().length > 0 && c.view() === "list"}>
        {() => (
          <List<InstalledSkill>
            each={c.filtered()}
            estimatedItemSize={58}
            class="flex-1"
            contentClass="gap-2 px-6 pb-6"
            testID="library-list"
          >
            {(skill) => (
              <SkillListRow
                {...itemProps}
                skill={skill()}
                selected={c.selectedIds().has(skill().id)}
                busy={c.busy()}
              />
            )}
          </List>
        )}
      </If>

      <If when={c.filtered().length > 0 && c.view() === "card"}>
        {() => (
          <List<InstalledSkill>
            each={c.filtered()}
            numColumns={cardColumnsFor(windowSize().width)}
            estimatedItemSize={120}
            class="flex-1"
            contentClass="gap-3 px-6 pb-6"
            testID="library-grid"
          >
            {(skill) => (
              <SkillCard
                {...itemProps}
                skill={skill()}
                selected={c.selectedIds().has(skill().id)}
                busy={c.busy()}
              />
            )}
          </List>
        )}
      </If>

      <SkillDetailModal store={props.store} controller={c} />
      <LibraryDialogs store={props.store} controller={c} />
    </Page>
  );
}
