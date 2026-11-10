import { computed, effect, signal } from "nsh";
import { Scroll, View } from "nsh";
import * as api from "../../api";
import { Banner, type BannerTone } from "../../components/ui";
import { describeError, toAppError } from "../../errors";
import { translate } from "../../i18n";
import type { AppStore } from "../../store";
import type {
  AppError,
  CheckUpdatesReport,
  DeleteReport,
  Installation,
  InstallationDetail,
  OperationKind,
  OperationSnapshot,
  UpdateConflictPolicy,
} from "../../types";
import {
  clampPage,
  installationById,
  pageCount,
  pageRange,
  paginate,
} from "../marketplace/model";
import { SkillCard } from "./components/SkillCard";
import { SkillDeleteDialog } from "./components/SkillDeleteDialog";
import { SkillDetailDialog } from "./components/SkillDetailDialog";
import { SkillEmptyState } from "./components/SkillEmptyState";
import { SkillListItem } from "./components/SkillListItem";
import { SkillOverwriteDialog } from "./components/SkillOverwriteDialog";
import { SkillPager } from "./components/SkillPager";
import { SkillProgressBar } from "./components/SkillProgressBar";
import { SkillRenameDialog } from "./components/SkillRenameDialog";
import { SkillSelectionHeader } from "./components/SkillSelectionHeader";
import { SkillToolbar } from "./components/SkillToolbar";
import { createText } from "./labels";
import {
  CARD_COLUMNS,
  allSelected,
  chunkItems,
  countForFilter,
  filterInstallations,
  selectedInstallations,
  someSelected,
  type StatusFilter,
  type ViewMode,
} from "./model";

const managedKinds: OperationKind[] = ["check_updates", "apply_update", "delete", "rename"];

interface Notice {
  tone: BannerTone;
  message: string;
}

function isTerminal(snapshot: OperationSnapshot): boolean {
  return (
    snapshot.state === "succeeded" || snapshot.state === "failed" || snapshot.state === "cancelled"
  );
}

export function MySkillsPage(props: { store: AppStore }) {
  const text = createText(props.store);

  const query = signal("");
  const filter = signal<StatusFilter>("all");
  const view = signal<ViewMode>("list");
  const page = signal(1);
  const selectedIds = signal<ReadonlySet<string>>(new Set());
  const detailId = signal<string | null>(null);
  const detail = signal<InstallationDetail | null>(null);
  const detailLoading = signal(false);
  const detailError = signal<AppError | null>(null);
  const renameTarget = signal<Installation | null>(null);
  const renameValue = signal("");
  const renameError = signal<string | null>(null);
  const deleteIds = signal<string[] | null>(null);
  const updateTarget = signal<Installation | null>(null);
  const operationId = signal<string | null>(null);
  const notice = signal<Notice | null>(null);

  const installations = computed(() => props.store.installations());
  const filtered = computed(() => filterInstallations(installations(), query(), filter()));
  const totalPages = computed(() => pageCount(filtered().length));
  const currentPage = computed(() => clampPage(page(), totalPages()));
  const visible = computed(() => paginate(filtered(), currentPage()));
  const cardRows = computed(() => chunkItems(visible()));
  const visibleIds = computed(() => visible().map((item) => item.id));
  const selected = computed(() => selectedInstallations(installations(), selectedIds()));
  const allVisibleSelected = computed(() => allSelected(visibleIds(), selectedIds()));
  const someVisibleSelected = computed(() => someSelected(visibleIds(), selectedIds()));
  const filterCounts = computed(() => {
    const list = installations();
    const counts: Record<StatusFilter, number> = {
      all: list.length,
      clean: countForFilter(list, "clean"),
      modified: countForFilter(list, "modified"),
      missing: countForFilter(list, "missing"),
      update_available: countForFilter(list, "update_available"),
    };
    return counts;
  });
  const detailInstallation = computed(() => {
    const value = detail();
    if (value !== null) {
      return value.installation;
    }
    const id = detailId();
    return id === null ? null : installationById(installations(), id);
  });
  const activeManaged = computed<OperationSnapshot | null>(() => {
    const active = props.store.activeOperation();
    return active !== null && managedKinds.includes(active.kind) ? active : null;
  });
  const progressOperation = computed<OperationSnapshot | null>(() => {
    const active = activeManaged();
    if (active !== null) {
      return active;
    }
    const id = operationId();
    if (id === null) {
      return null;
    }
    const snapshot = props.store.operation(id);
    if (snapshot === null || !managedKinds.includes(snapshot.kind)) {
      return null;
    }
    return snapshot.state === "queued" || snapshot.state === "running" ? snapshot : null;
  });
  const busy = computed(() => activeManaged() !== null);
  const deleteItems = computed(() => {
    const ids = deleteIds();
    if (ids === null) {
      return [];
    }
    return installations().filter((item) => ids.includes(item.id));
  });
  const deleteModifiedCount = computed(
    () => deleteItems().filter((item) => item.local_state === "modified").length,
  );
  const rangeSummary = computed(() => {
    const range = pageRange(currentPage(), filtered().length);
    if (!range) {
      return text("myskills.rangeEmpty");
    }
    return text("myskills.range", {
      start: range.start,
      end: range.end,
      total: filtered().length,
    });
  });

  let detailSequence = 0;
  let settledOperationId: string | null = null;

  function errorText(error: AppError): string {
    return describeError(error, translate(props.store.settings()?.language ?? "zh_cn"));
  }

  function showError(prefix: string, error: unknown): void {
    notice.set({ tone: "error", message: `${prefix}: ${errorText(toAppError(error))}` });
  }

  function clearSelection(): void {
    selectedIds.set(new Set());
  }

  function toggle(id: string, checked: boolean): void {
    const next = new Set(selectedIds());
    if (checked) {
      next.add(id);
    } else {
      next.delete(id);
    }
    selectedIds.set(next);
  }

  function toggleAll(checked: boolean): void {
    const next = new Set(selectedIds());
    for (const id of visibleIds()) {
      if (checked) {
        next.add(id);
      } else {
        next.delete(id);
      }
    }
    selectedIds.set(next);
  }

  function selectFilter(next: StatusFilter): void {
    if (next === filter()) {
      return;
    }
    filter.set(next);
    page.set(1);
  }

  async function loadDetail(id: string): Promise<void> {
    const request = ++detailSequence;
    detailLoading.set(true);
    detailError.set(null);
    try {
      const result = await api.installationDetail(id);
      if (request !== detailSequence) {
        return;
      }
      detail.set(result);
    } catch (error) {
      if (request !== detailSequence) {
        return;
      }
      detailError.set(toAppError(error));
    } finally {
      if (request === detailSequence) {
        detailLoading.set(false);
      }
    }
  }

  function openDetail(installation: Installation): void {
    detail.set(null);
    detailError.set(null);
    detailId.set(installation.id);
  }

  function closeDetail(): void {
    detailSequence += 1;
    detailId.set(null);
  }

  function finishCloseDetail(): void {
    if (detailId() !== null) {
      return;
    }
    detail.set(null);
    detailError.set(null);
    detailLoading.set(false);
  }

  function refreshDetail(): void {
    const id = detailId();
    if (id !== null) {
      void loadDetail(id);
    }
  }

  function openRename(installation: Installation): void {
    closeDetail();
    renameError.set(null);
    renameValue.set(installation.display_name);
    renameTarget.set(installation);
  }

  function closeRename(): void {
    renameTarget.set(null);
    renameError.set(null);
  }

  function submitRename(): void {
    const target = renameTarget();
    if (target === null) {
      return;
    }
    const name = renameValue().trim();
    if (name === "") {
      renameError.set(text("myskills.rename.invalid"));
      return;
    }
    if (name === target.display_name) {
      closeRename();
      return;
    }
    notice.set(null);
    try {
      operationId.set(api.renameInstallation(target.id, name));
      closeRename();
    } catch (error) {
      renameError.set(errorText(toAppError(error)));
    }
  }

  function requestDelete(ids: string[]): void {
    if (ids.length === 0 || busy()) {
      return;
    }
    closeDetail();
    deleteIds.set(ids);
  }

  function closeDelete(): void {
    deleteIds.set(null);
  }

  function confirmDelete(): void {
    const ids = deleteIds();
    if (ids === null) {
      return;
    }
    const items = installations().filter((item) => ids.includes(item.id));
    const confirmModified = items.some((item) => item.local_state === "modified");
    notice.set(null);
    try {
      operationId.set(api.deleteInstallations(ids, confirmModified));
      closeDelete();
    } catch (error) {
      closeDelete();
      showError(text("myskills.delete.title"), error);
    }
  }

  function startUpdate(installation: Installation, policy: UpdateConflictPolicy): void {
    notice.set(null);
    try {
      operationId.set(api.applyUpdate(installation.id, policy));
    } catch (error) {
      showError(text("myskills.update"), error);
    }
  }

  function requestUpdate(installation: Installation): void {
    if (busy()) {
      return;
    }
    closeDetail();
    if (installation.local_state === "modified") {
      updateTarget.set(installation);
      return;
    }
    startUpdate(installation, "reject");
  }

  function closeOverwrite(): void {
    updateTarget.set(null);
  }

  function confirmOverwrite(): void {
    const target = updateTarget();
    if (target === null) {
      return;
    }
    closeOverwrite();
    startUpdate(target, "overwrite");
  }

  function startCheck(ids: string[]): void {
    if (ids.length === 0 || busy()) {
      return;
    }
    notice.set(null);
    try {
      operationId.set(api.checkUpdates(ids));
    } catch (error) {
      showError(text("myskills.checkAll"), error);
    }
  }

  function checkAll(): void {
    startCheck(installations().map((item) => item.id));
  }

  function checkOne(installation: Installation): void {
    closeDetail();
    startCheck([installation.id]);
  }

  function cancelOperation(snapshot: OperationSnapshot): void {
    try {
      api.cancelOperation(snapshot.id);
    } catch (error) {
      showError(text("myskills.progress.cancel"), error);
    }
  }

  function localModifiedTarget(snapshot: OperationSnapshot): Installation | null {
    const error = snapshot.error === null ? null : toAppError(snapshot.error);
    if (error === null || error.code !== "LOCAL_MODIFIED") {
      return null;
    }
    const details = error.details;
    if (typeof details !== "object" || details === null) {
      return null;
    }
    const id = (details as Record<string, unknown>).installationId;
    return typeof id === "string" ? installationById(installations(), id) : null;
  }

  function handleSettled(snapshot: OperationSnapshot): void {
    if (snapshot.state === "cancelled") {
      notice.set({ tone: "warning", message: text("myskills.notice.cancelled") });
      return;
    }
    if (snapshot.state === "failed") {
      const target = localModifiedTarget(snapshot);
      if (target !== null) {
        updateTarget.set(target);
        notice.set({
          tone: "warning",
          message: text("myskills.notice.localModified", { name: target.display_name }),
        });
        return;
      }
      const error = snapshot.error === null ? null : toAppError(snapshot.error);
      notice.set({
        tone: "error",
        message:
          error === null
            ? text("myskills.progress.working")
            : `${text("myskills.progress.working")}: ${errorText(error)}`,
      });
      return;
    }

    switch (snapshot.kind) {
      case "check_updates": {
        const report = snapshot.result as CheckUpdatesReport | null;
        const checked = report?.checked ?? [];
        const failed = report?.failed.length ?? 0;
        const available = checked.filter((item) => item.update_state === "available").length;
        const base =
          available === 0
            ? text("myskills.notice.checkNone")
            : text("myskills.notice.checkDone", { count: available });
        const message =
          failed === 0
            ? base
            : `${base}${text("myskills.notice.checkFailed", { count: failed })}`;
        notice.set({
          tone: failed === 0 ? "success" : "warning",
          message,
        });
        return;
      }
      case "delete": {
        const report = snapshot.result as DeleteReport | null;
        const deleted = report?.deleted.length ?? 0;
        const failed = report?.failed.length ?? 0;
        clearSelection();
        notice.set({
          tone: failed === 0 ? "success" : "warning",
          message:
            failed === 0
              ? text("myskills.notice.deleteDone", { count: deleted })
              : text("myskills.notice.deletePartial", { count: deleted, failed }),
        });
        return;
      }
      case "rename": {
        const result = snapshot.result as Installation | null;
        notice.set({
          tone: "success",
          message: text("myskills.notice.renamed", { name: result?.display_name ?? "" }),
        });
        return;
      }
      case "apply_update": {
        const result = snapshot.result as Installation | null;
        notice.set({
          tone: "success",
          message: text("myskills.notice.updated", { name: result?.display_name ?? "" }),
        });
        return;
      }
      default:
        return;
    }
  }

  function operationPhase(snapshot: OperationSnapshot): string {
    if (snapshot.state === "queued") {
      return text("myskills.progress.queued");
    }
    switch (snapshot.kind) {
      case "check_updates":
        if (snapshot.progress?.current !== undefined && snapshot.progress.total !== undefined) {
          return translate(props.store.settings()?.language ?? "zh_cn")(
            "myskills.progress.checkingItem",
            {
              current: snapshot.progress.current,
              total: snapshot.progress.total,
            },
          );
        }
        return text("myskills.progress.checking");
      case "delete":
        return text("myskills.progress.deleting");
      case "rename":
        return text("myskills.progress.renaming");
      case "apply_update":
        switch (snapshot.progress?.phase) {
          case "downloading":
            return text("myskills.progress.downloading");
          case "validating":
            return text("myskills.progress.validating");
          case "installing":
            return text("myskills.progress.installing");
          default:
            return text("myskills.progress.working");
        }
      default:
        return text("myskills.progress.working");
    }
  }

  function canCancel(snapshot: OperationSnapshot): boolean {
    return snapshot.state === "queued" || snapshot.progress?.cancellable === true;
  }

  effect(() => {
    const id = operationId();
    if (id === null) {
      return;
    }
    const snapshot = props.store.operation(id);
    if (snapshot === null || !isTerminal(snapshot) || settledOperationId === snapshot.id) {
      return;
    }
    settledOperationId = snapshot.id;
    operationId.set(null);
    handleSettled(snapshot);
  });

  effect(() => {
    const id = detailId();
    if (id === null) {
      return;
    }
    if (!installations().some((item) => item.id === id)) {
      closeDetail();
      return;
    }
    void loadDetail(id);
  });

  function renderCardRow(row: () => Installation[]) {
    return (
      <View class="flex-row items-stretch gap-2">
        <Each of={row()}>
          {(installation) => (
            <SkillCard
              installation={installation}
              selected={() => selectedIds().has(installation().id)}
              busy={() => busy()}
              text={text}
              onToggle={(checked) => toggle(installation().id, checked)}
              onDetail={() => openDetail(installation())}
              onRename={() => openRename(installation())}
              onUpdate={() => requestUpdate(installation())}
              onDelete={() => requestDelete([installation().id])}
            />
          )}
        </Each>
        <If when={row().length < CARD_COLUMNS}>
          {() => (
            <Each of={Array.from({ length: CARD_COLUMNS - row().length })} key="index">
              {() => <View class="flex-1" />}
            </Each>
          )}
        </If>
      </View>
    );
  }

  return (
    <View class="flex-1 bg-background">
      <SkillToolbar
        text={text}
        query={query}
        filter={filter}
        counts={filterCounts}
        total={() => installations().length}
        view={view}
        selectedCount={() => selected().length}
        busy={busy}
        onQueryChange={(value) => {
          query.set(value);
          page.set(1);
        }}
        onFilterChange={selectFilter}
        onViewChange={(mode) => view.set(mode)}
        onCheckAll={checkAll}
        onClearSelection={clearSelection}
        onDeleteSelected={() => requestDelete(selected().map((item) => item.id))}
      />

      <View class="gap-2 px-6 pt-4">
        <If when={props.store.installationsError()}>
          {(error) => <Banner tone="warning" message={errorText(error())} />}
        </If>
        <If when={notice()}>
          {(value) => (
            <Banner
              tone={value().tone}
              message={value().message}
              actionLabel={text("myskills.notice.dismiss")}
              onAction={() => notice.set(null)}
            />
          )}
        </If>
        <If when={progressOperation()}>
          {(snapshot) => (
            <SkillProgressBar
              snapshot={snapshot}
              phase={operationPhase}
              canCancel={canCancel}
              cancelLabel={text("myskills.progress.cancel")}
              onCancel={cancelOperation}
            />
          )}
        </If>
      </View>

      <If when={installations().length === 0}>
        {() => (
          <SkillEmptyState
            text={text}
            hasInstallations={() => false}
            hasQuery={() => query().trim() !== ""}
          />
        )}
      </If>

      <If when={installations().length > 0 && filtered().length === 0}>
        {() => (
          <SkillEmptyState
            text={text}
            hasInstallations={() => true}
            hasQuery={() => query().trim() !== ""}
          />
        )}
      </If>

      <If when={filtered().length > 0}>
        {() => (
          <Scroll class="flex-1">
            <View class="gap-2 p-6">
              <SkillSelectionHeader
                text={text}
                selectedCount={() => selected().length}
                allSelected={allVisibleSelected}
                someSelected={someVisibleSelected}
                disabled={() => busy() || visibleIds().length === 0}
                onChange={toggleAll}
              />
              <If when={view() === "list"}>
                {() => (
                  <Each of={visible()}>
                    {(installation) => (
                      <SkillListItem
                        installation={installation}
                        selected={() => selectedIds().has(installation().id)}
                        busy={() => busy()}
                        text={text}
                        onToggle={(checked) => toggle(installation().id, checked)}
                        onDetail={() => openDetail(installation())}
                        onRename={() => openRename(installation())}
                        onUpdate={() => requestUpdate(installation())}
                        onDelete={() => requestDelete([installation().id])}
                      />
                    )}
                  </Each>
                )}
              </If>
              <If when={view() === "card"}>
                {() => (
                  <Each of={cardRows()}>
                    {(row) => renderCardRow(row)}
                  </Each>
                )}
              </If>
            </View>
          </Scroll>
        )}
      </If>

      <If when={filtered().length > 0}>
        {() => (
          <SkillPager
            text={text}
            summary={rangeSummary}
            page={currentPage}
            totalPages={totalPages}
            onChange={(value) => page.set(value)}
          />
        )}
      </If>

      <SkillDetailDialog
        open={() => detailId() !== null}
        installation={detailInstallation}
        detail={detail}
        loading={detailLoading}
        error={detailError}
        busy={busy}
        text={text}
        errorText={errorText}
        onClose={closeDetail}
        onClosed={finishCloseDetail}
        onRefresh={refreshDetail}
        onCheck={checkOne}
        onUpdate={requestUpdate}
        onRename={openRename}
        onDelete={(item) => requestDelete([item.id])}
      />
      <SkillRenameDialog
        open={() => renameTarget() !== null}
        target={renameTarget}
        value={renameValue}
        error={renameError}
        text={text}
        onChange={(value) => {
          renameValue.set(value);
          renameError.set(null);
        }}
        onSubmit={submitRename}
        onClose={closeRename}
      />
      <SkillDeleteDialog
        open={() => deleteIds() !== null}
        items={deleteItems}
        modifiedCount={deleteModifiedCount}
        text={text}
        onConfirm={confirmDelete}
        onClose={closeDelete}
      />
      <SkillOverwriteDialog
        open={() => updateTarget() !== null}
        target={updateTarget}
        text={text}
        onConfirm={confirmOverwrite}
        onClose={closeOverwrite}
      />
    </View>
  );
}
