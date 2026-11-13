import { Scroll, View } from "nsh";
import { Icon } from "../../../components/icon";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  EmptyState,
  Label,
  Toolbar,
  cx,
} from "../../../components/ui";
import type { AppStore } from "../../../store";
import type { ImportController } from "../import-controller";
import { ImportCandidateRow } from "./ImportCandidateRow";

/** The local import tab: the Agent directories' skills plus any chosen folder or ZIP. */
export function LocalImportPanel(props: { store: AppStore; controller: ImportController }) {
  const text = props.store.text;
  const c = props.controller;

  return (
    <View class="flex-1 pt-3">
      <Toolbar>
        <Checkbox
          checked={c.localAllSelected()}
          indeterminate={c.localSomeSelected()}
          disabled={c.localCandidates().length === 0}
          label={text("import.selectAll")}
          onChange={c.toggleAllLocal}
          testID="import-local-select-all"
        />
        <Label tone="muted">{text("import.local.found", { count: c.localCandidates().length })}</Label>
        <View class="flex-1" />
        <Button
          label={text("import.local.scan")}
          icon="refresh"
          variant="ghost"
          onPress={c.rescanLocal}
          testID="import-local-scan"
        />
        <Button
          label={text("import.local.chooseFolder")}
          icon="folder"
          onPress={c.chooseFolder}
          testID="import-local-folder"
        />
        <Button
          label={text("import.local.chooseZip")}
          icon="package"
          onPress={c.chooseZip}
          testID="import-local-zip"
        />
        <Button
          label={text("import.action", { count: c.localSelected() })}
          icon="download"
          variant="primary"
          disabled={c.localSelected() === 0}
          onPress={() => c.importSelected("local")}
          testID="import-local-submit"
        />
      </Toolbar>

      <If when={c.localCandidates().length === 0}>
        {() => (
          <EmptyState
            icon="folder"
            title={text("import.local.empty")}
            hint={text("import.local.emptyHint")}
          />
        )}
      </If>

      <If when={c.localCandidates().length > 0}>
        {() => (
          <Scroll class="flex-1" testID="import-local-list">
            <View class="gap-3 px-6 pb-6">
              <Each of={c.localGroups()}>
                {(group) => (
                  <Card>
                    <View
                      class={cx(
                        "flex-row items-center gap-3 px-4 py-3 hover:bg-accent",
                        c.collapsedGroups().has(group().key)
                          ? "rounded-(--radius)"
                          : "rounded-t-(--radius)",
                      )}
                      onPress={() => c.toggleGroup(group().key)}
                      testID={`import-local-group-${group().key}`}
                    >
                      <Icon name="folder" size={18} />
                      <View class="flex-1 gap-0.5">
                        <View class="flex-row items-center gap-2">
                          <Label variant="strong">{group().label}</Label>
                          <Badge
                            label={text("import.local.groupCount", {
                              count: group().candidates.length,
                            })}
                          />
                        </View>
                        <Label variant="caption" tone="muted" lines={1}>
                          {group().roots.length === 1
                            ? (group().roots[0] ?? "")
                            : text("import.local.groupRoots", { count: group().roots.length })}
                        </Label>
                      </View>
                      <Icon
                        name={
                          c.collapsedGroups().has(group().key) ? "chevronRight" : "chevronDown"
                        }
                      />
                    </View>

                    <If when={!c.collapsedGroups().has(group().key)}>
                      {() => (
                        <View class="gap-1 border-t border-border p-2">
                          <Each of={group().candidates}>
                            {(candidate) => (
                              <ImportCandidateRow
                                candidate={candidate()}
                                selected={c.localSelectedIds().has(candidate().id)}
                                text={text}
                                plain
                                onToggle={(checked) => c.toggleLocal(candidate().id, checked)}
                              />
                            )}
                          </Each>
                        </View>
                      )}
                    </If>
                  </Card>
                )}
              </Each>
            </View>
          </Scroll>
        )}
      </If>
    </View>
  );
}
