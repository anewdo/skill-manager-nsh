import { List, View } from "nsh";
import { Icon } from "../../../components/icon";
import {
  Badge,
  Banner,
  Button,
  Checkbox,
  EmptyState,
  Label,
  ListRow,
  Modal,
  Notices,
  Spinner,
  Toolbar,
} from "../../../components/ui";
import type { LocalImportItem } from "../../../lib/types";
import type { AppStore } from "../../../store";
import type { ImportController } from "../import-controller";
import type { LocalImportRow } from "../import-model";
import { ImportCandidateRow } from "./ImportCandidateRow";

export function LocalImportPanel(props: { store: AppStore; controller: ImportController }) {
  const text = props.store.text;
  const c = props.controller;

  return (
    <View
      class="flex-1 pt-3"
      onFileDrop={c.localBusy() ? undefined : (event) => c.dropLocal(event.uris)}
      testID="import-local-drop"
    >
      <Toolbar>
        <Checkbox
          checked={c.localAllSelected()}
          indeterminate={c.localSomeSelected()}
          disabled={c.localBusy() || c.localCandidates().length === 0}
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
          disabled={c.localBusy() || c.localDiscovery().sources.length === 0}
          onPress={c.rescanLocal}
          testID="import-local-scan"
        />
        <Button
          label={text("import.local.chooseFolder")}
          icon="folder"
          disabled={c.localBusy()}
          onPress={c.chooseFolder}
          testID="import-local-folder"
        />
        <Button
          label={text("import.local.chooseZip")}
          icon="package"
          disabled={c.localBusy()}
          onPress={c.chooseZip}
          testID="import-local-zip"
        />
        <Button
          label={text("import.action", { count: c.localSelected() })}
          icon="download"
          variant="primary"
          disabled={c.localBusy() || c.localSelected() === 0}
          onPress={c.importLocalSelected}
          testID="import-local-submit"
        />
      </Toolbar>

      <View class="gap-2 pb-3">
        <Notices>
          <If when={c.localHasZip()}>
            {() => (
              <Banner
                tone="info"
                message={text("import.local.zipSession")}
                testID="import-local-zip-session"
              />
            )}
          </If>
          <If when={c.localLoading()}>
            {() => (
              <View class="flex-row items-center gap-2">
                <Spinner />
                <Label tone="muted">{text("import.local.discovering")}</Label>
              </View>
            )}
          </If>
          <If when={c.localPendingId()}>
            {() => (
              <Banner tone="info" message={text("import.local.running")}>
                <Button
                  label={text("common.cancel")}
                  size="sm"
                  variant="ghost"
                  onPress={c.cancelLocal}
                  testID="import-local-cancel"
                />
              </Banner>
            )}
          </If>
          <If when={c.localSummary()}>
            {(summary) => (
              <Banner tone="info" message={summary()} testID="import-local-summary">
                <Button
                  label={text("import.local.results")}
                  size="sm"
                  variant="ghost"
                  onPress={() => c.localReportOpen.set(true)}
                  testID="import-local-results"
                />
              </Banner>
            )}
          </If>
          <If when={c.localDiscovery().sources.length > 0 || c.localDiscovery().issues.length > 0}>
            {() => (
              <View class="flex-row justify-end">
                <Button
                  label={text("import.local.discard")}
                  size="sm"
                  variant="ghost"
                  disabled={c.localBusy()}
                  onPress={c.discardLocal}
                  testID="import-local-discard"
                />
              </View>
            )}
          </If>
        </Notices>
      </View>

      <If when={c.localRows().length === 0 && !c.localLoading()}>
        {() => (
          <EmptyState
            icon="folder"
            title={text("import.local.empty")}
            hint={text("import.local.emptyHint")}
          />
        )}
      </If>

      <If when={c.localRows().length > 0}>
        {() => (
          <List<LocalImportRow>
            each={c.localRows()}
            keyExtractor={(row) => row.id}
            estimatedItemSize={88}
            class="flex-1"
            contentClass="gap-1 px-6 pb-6"
            testID="import-local-list"
          >
            {(row) => (
              <View>
                <If when={row().group}>
                  {(group) => (
                    <ListRow
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
                        name={c.collapsedGroups().has(group().key) ? "chevronRight" : "chevronDown"}
                      />
                    </ListRow>
                  )}
                </If>
                <If when={row().candidate}>
                  {(candidate) => (
                    <ImportCandidateRow
                      candidate={candidate()}
                      selected={c.localSelectedIds().has(candidate().id)}
                      disabled={c.localBusy()}
                      text={text}
                      plain
                      onToggle={(checked) => c.toggleLocal(candidate().id, checked)}
                    />
                  )}
                </If>
                <If when={row().source}>
                  {(source) => (
                    <Banner
                      tone="info"
                      message={`${source().path}: ${text(
                        source().status === "invalid"
                          ? "import.local.allInvalid"
                          : "import.local.sourceEmpty",
                      )}`}
                    />
                  )}
                </If>
                <If when={row().issue}>
                  {(issue) => (
                    <Banner
                      tone="warning"
                      message={`${issue().path}: ${props.store.errorText(issue().error)}`}
                    />
                  )}
                </If>
              </View>
            )}
          </List>
        )}
      </If>

      <Modal
        open={c.localReportOpen()}
        title={text("import.local.results")}
        closeLabel={text("common.close")}
        onClose={() => c.localReportOpen.set(false)}
        size="lg"
        footer={
          <Button label={text("common.close")} onPress={() => c.localReportOpen.set(false)} />
        }
      >
        <List<LocalImportItem>
          each={c.localReport()?.items ?? []}
          keyExtractor={(item) => item.candidate_id}
          estimatedItemSize={100}
          class="h-[360px]"
          contentClass="gap-2"
          testID="import-local-result-list"
        >
          {(item) => (
            <ListRow>
              <View class="flex-1 gap-1">
                <View class="flex-row items-center gap-2">
                  <Label variant="strong">{item().name}</Label>
                  <Badge
                    label={text(`import.local.status.${item().status}`)}
                    tone={item().status === "failed"
                      ? "danger"
                      : item().status === "cancelled" ? "neutral" : "success"}
                  />
                </View>
                <Label variant="caption" tone="muted">{item().path}</Label>
                <If when={item().skill}>
                  {(skill) => (
                    <Label variant="caption">
                      {text("import.local.finalSkill", {
                        name: skill().display_name,
                        path: skill().root_relpath,
                      })}
                    </Label>
                  )}
                </If>
                <If when={item().error}>
                  {(error) => (
                    <Label variant="caption" tone="danger">{props.store.errorText(error())}</Label>
                  )}
                </If>
              </View>
            </ListRow>
          )}
        </List>
      </Modal>
    </View>
  );
}
