import { List, View } from "nsh";
import { Badge, Banner, Button, Checkbox, EmptyState, Label, ListRow, Modal, Notices, Spinner, TextField, Toolbar } from "../../../components/ui";
import type { MessageKey } from "../../../i18n";
import type { AppError, LocalImportItem, UrlCandidate } from "../../../lib/types";
import type { AppStore } from "../../../store";
import type { UrlImportController } from "../url-import-controller";

function errorKey(error: AppError): MessageKey | null {
  const reasons: Record<string, MessageKey> = {
    rate_limited: "import.url.error.rateLimited",
    authentication_required: "import.url.error.authentication",
    repository_not_found: "import.url.error.notFound",
    reference_not_found: "import.url.error.reference",
    ambiguous_reference: "import.url.error.ambiguous",
    path_not_found: "import.url.error.path",
    discovery_expired: "import.url.error.expired",
    network: "import.url.error.network",
  };
  const reason = (error.details as { reason?: string } | undefined)?.reason;
  return reason === undefined ? null : reasons[reason] ?? null;
}

export function UrlImportPanel(props: { store: AppStore; controller: UrlImportController; onOpenLibrary: (id: string) => void }) {
  const text = props.store.text;
  const c = props.controller;
  return (
    <View class="flex-1 pt-3" testID="import-url-panel">
      <Toolbar>
        <If when={c.pendingId() === null}>
          {() => <TextField class="flex-1" icon="github" value={c.query()} onChange={c.editQuery}
            onSubmit={() => void c.discover()} placeholder={text("import.url.placeholder")}
            clearLabel={text("common.clear")} testID="import-url-input" />}
        </If>
        <If when={c.pendingId() !== null}>
          {() => <Label class="flex-1" selectable>{c.query()}</Label>}
        </If>
        <Button label={text("import.url.discover")} icon="search" disabled={c.busy() || c.query().trim() === ""}
          onPress={() => void c.discover()} testID="import-url-discover" />
        <Button label={text("import.action", { count: c.selected().length })} icon="download" variant="primary"
          disabled={c.busy() || c.selected().length === 0} onPress={c.importSelected} testID="import-url-submit" />
      </Toolbar>
      <Notices>
        <If when={c.loading()}>
          {() => <View class="flex-row items-center gap-2"><Spinner /><Label tone="muted">{text("import.url.discovering")}</Label></View>}
        </If>
        <If when={c.error()}>
          {(error) => <Banner tone="danger" message={errorKey(error()) === null ? props.store.errorText(error()) : text(errorKey(error())!)} testID="import-url-error">
            <Button label={text("common.retry")} variant="ghost" size="sm" onPress={() => void c.discover()} />
          </Banner>}
        </If>
        <If when={c.pendingId()}>
          {() => <Banner tone="info" message={text("import.url.running")}>
            <Button label={text("common.cancel")} variant="ghost" size="sm" onPress={c.cancel} testID="import-url-cancel" />
          </Banner>}
        </If>
        <If when={c.summary()}>
          {(summary) => <Banner tone="info" message={summary()} testID="import-url-summary">
            <Button label={text("import.local.results")} variant="ghost" size="sm" onPress={() => c.reportOpen.set(true)} testID="import-url-results" />
          </Banner>}
        </If>
        <If when={c.discovery()}>
          {(discovery) => <View class="gap-1">
            <Label variant="caption" tone="muted" selectable>{`${discovery().repository} · ${discovery().git_ref} · ${discovery().revision.slice(0, 12)}`}</Label>
            <Label variant="caption" tone="muted">{text("import.url.snapshotHint")}</Label>
            <Each of={discovery().issues}>
              {(issue) => <Banner tone="warning" message={`${issue().path || "/"}: ${props.store.errorText(issue().error)}`} />}
            </Each>
          </View>}
        </If>
      </Notices>
      <If when={c.discovery() !== null && c.candidates().length === 0}>
        {() => <EmptyState icon="github" title={text("import.url.empty")} hint={text("import.url.emptyHint")} />}
      </If>
      <If when={c.discovery() === null && !c.loading() && c.error() === null && c.report() === null}>
        {() => <EmptyState icon="github" title={text("import.url.initial")} hint={text("import.url.initialHint")} />}
      </If>
      <If when={c.candidates().length > 0}>
        {() => <>
          <Toolbar>
            <Checkbox checked={c.allSelected()} indeterminate={c.someSelected()} disabled={c.busy()}
              label={text("import.selectAll")} onChange={c.toggleAll} testID="import-url-select-all" />
            <Label tone="muted">{text("import.local.found", { count: c.candidates().length })}</Label>
          </Toolbar>
          <List<UrlCandidate> each={c.candidates()} keyExtractor={(candidate) => candidate.id}
            estimatedItemSize={108} class="flex-1" contentClass="gap-1 px-6 pb-6" testID="import-url-list">
            {(candidate) => <ListRow>
              <Checkbox checked={c.selectedIds().has(candidate().id)} disabled={c.busy()}
                label={candidate().name}
                onChange={(checked) => c.toggle(candidate().id, checked)} testID={`import-url-candidate-${candidate().id}`} />
              <View class="flex-1 gap-1">
                <Label variant="strong">{candidate().name}</Label>
                <Label tone="muted" lines={2}>{candidate().description}</Label>
                <Label variant="caption" tone="muted" selectable>{candidate().path || "/"}</Label>
                <If when={candidate().existing}>
                  {(skill) => <Label variant="caption">{text("import.url.existing", { name: skill().display_name, path: skill().root_relpath })}</Label>}
                </If>
              </View>
              <If when={candidate().existing}>
                {(skill) => <Button label={text("import.url.openUpdate")} variant="ghost" size="sm"
                  disabled={c.busy()} onPress={() => props.onOpenLibrary(skill().id)} />}
              </If>
            </ListRow>}
          </List>
        </>}
      </If>
      <Modal open={c.reportOpen()} title={text("import.local.results")} size="lg"
        footer={<Button label={text("common.close")} onPress={() => c.reportOpen.set(false)} />}
        closeLabel={text("common.close")} onClose={() => c.reportOpen.set(false)}>
        <List<LocalImportItem> each={c.report()?.items ?? []} keyExtractor={(item) => item.candidate_id}
          estimatedItemSize={104} class="h-[360px]" contentClass="gap-2" testID="import-url-result-list">
          {(item) => <ListRow>
            <View class="flex-1 gap-1">
              <View class="flex-row items-center gap-2">
                <Label variant="strong">{item().name}</Label>
                <Badge label={text(`import.local.status.${item().status}`)} tone={item().status === "failed" ? "danger" : item().status === "cancelled" ? "neutral" : "success"} />
              </View>
              <Label variant="caption" tone="muted">{item().path || "/"}</Label>
              <If when={item().skill}>
                {(skill) => <Label variant="caption">{text("import.local.finalSkill", { name: skill().display_name, path: skill().root_relpath })}</Label>}
              </If>
              <If when={item().error}>{(error) => <Label variant="caption" tone="danger">{props.store.errorText(error())}</Label>}</If>
            </View>
            <If when={item().skill?.source}>
              {() => <Button label={text("import.url.openUpdate")} size="sm" variant="ghost"
                onPress={() => { c.reportOpen.set(false); props.onOpenLibrary(item().skill!.id); }} />}
            </If>
          </ListRow>}
        </List>
      </Modal>
    </View>
  );
}
