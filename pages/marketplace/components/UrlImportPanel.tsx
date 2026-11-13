import { List, View } from "nsh";
import { Button, Checkbox, EmptyState, Label, TextField, Toolbar } from "../../../components/ui";
import type { ImportCandidate } from "../../../lib/types";
import type { AppStore } from "../../../store";
import type { ImportController } from "../import-controller";
import { ImportCandidateRow } from "./ImportCandidateRow";

/** The URL import tab: a GitHub address, the skills it holds and the ones picked. */
export function UrlImportPanel(props: { store: AppStore; controller: ImportController }) {
  const text = props.store.text;
  const c = props.controller;

  return (
    <View class="flex-1 pt-3">
      <View class="gap-2 px-6 pb-3">
        <View class="flex-row items-center gap-2">
          <TextField
            class="flex-1"
            icon="globe"
            value={c.url()}
            placeholder={text("import.url.placeholder")}
            clearLabel={text("common.clear")}
            onChange={c.editUrl}
            onSubmit={c.discoverUrl}
            testID="import-url-input"
          />
          <Button
            label={text("import.url.discover")}
            icon="search"
            variant="primary"
            disabled={c.url().trim() === ""}
            onPress={c.discoverUrl}
            testID="import-url-discover"
          />
        </View>
        <If
          when={c.urlError()}
          else={
            <Label variant="caption" tone="muted">
              {text("import.url.hint")}
            </Label>
          }
        >
          {(error) => (
            <Label variant="caption" tone="danger">
              {error()}
            </Label>
          )}
        </If>
      </View>

      <If when={c.urlCandidates() === null}>
        {() => (
          <EmptyState
            icon="globe"
            title={text("import.url.empty")}
            hint={text("import.url.emptyHint")}
          />
        )}
      </If>

      <If when={c.urlCandidates() !== null && (c.urlCandidates()?.length ?? 0) === 0}>
        {() => <EmptyState icon="searchX" title={text("import.url.none")} />}
      </If>

      <If when={(c.urlCandidates()?.length ?? 0) > 0}>
        {() => (
          <Toolbar>
            <Checkbox
              checked={c.urlAllSelected()}
              indeterminate={c.urlSomeSelected()}
              label={text("import.selectAll")}
              onChange={c.toggleAllUrl}
              testID="import-url-select-all"
            />
            <Label tone="muted">
              {text("import.url.found", {
                count: c.urlCandidates()?.length ?? 0,
                repository: c.urlRepository() ?? "",
              })}
            </Label>
            <View class="flex-1" />
            <Button
              label={text("import.action", { count: c.urlSelected() })}
              icon="download"
              variant="primary"
              disabled={c.urlSelected() === 0}
              onPress={() => c.importSelected("url")}
              testID="import-url-submit"
            />
          </Toolbar>
        )}
      </If>

      <If when={(c.urlCandidates()?.length ?? 0) > 0}>
        {() => (
          <List<ImportCandidate>
            each={c.urlCandidates() ?? []}
            estimatedItemSize={74}
            class="flex-1"
            contentClass="gap-2 px-6 pb-6"
            testID="import-url-list"
          >
            {(candidate) => (
              <ImportCandidateRow
                candidate={candidate()}
                selected={c.urlSelectedIds().has(candidate().id)}
                text={text}
                onToggle={(checked) => c.toggleUrl(candidate().id, checked)}
              />
            )}
          </List>
        )}
      </If>
    </View>
  );
}
