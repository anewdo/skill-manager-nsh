import { computed, effect, signal } from "nsh";
import * as api from "../../lib/api";
import { toAppError } from "../../lib/errors";
import type { MessageKey } from "../../i18n";
import type {
  AppError,
  CheckUpdatesReport,
  DeleteReport,
  InstalledSkill,
  OperationKind,
  OperationSnapshot,
  SkillDetail,
  UpdateConflictPolicy,
} from "../../lib/types";
import type { AppStore } from "../../store";
import {
  allSelected,
  filterCounts,
  filterSkills,
  selectedSkills,
  skillById,
  someSelected,
  type StatusFilter,
  type ViewMode,
} from "./model";

const libraryKinds: OperationKind[] = ["check_updates", "apply_update", "delete", "rename"];

function isTerminal(snapshot: OperationSnapshot): boolean {
  return (
    snapshot.state === "succeeded" || snapshot.state === "failed" || snapshot.state === "cancelled"
  );
}

export type LibraryController = ReturnType<typeof createLibraryController>;

/**
 * The library page's state and what the user can do on it. Results are
 * reported as toasts; progress shows in the app-wide operations indicator.
 */
export function createLibraryController(store: AppStore) {
  const text = store.text;

  const query = signal("");
  const filter = signal<StatusFilter>("all");
  const view = signal<ViewMode>("list");
  const selectedIds = signal<ReadonlySet<string>>(new Set());

  const detailId = signal<string | null>(null);
  const detail = signal<SkillDetail | null>(null);
  const detailLoading = signal(false);
  const detailError = signal<AppError | null>(null);

  const renameTarget = signal<InstalledSkill | null>(null);
  const renameValue = signal("");
  const renameError = signal<string | null>(null);
  const deleteIds = signal<string[] | null>(null);
  const overwriteTarget = signal<InstalledSkill | null>(null);

  /** The operation this page started and has not reported yet. */
  const pendingId = signal<string | null>(null);

  const skills = store.skills;
  const filtered = computed(() => filterSkills(skills(), query(), filter()));
  const filteredIds = computed(() => filtered().map((skill) => skill.id));
  const counts = computed(() => filterCounts(skills()));
  const selected = computed(() => selectedSkills(skills(), selectedIds()));
  const allFilteredSelected = computed(() => allSelected(filteredIds(), selectedIds()));
  const someFilteredSelected = computed(() => someSelected(filteredIds(), selectedIds()));
  /** A library operation runs or waits: the page starts no other meanwhile. */
  const busy = computed(() => {
    const active = store.activeOperation();
    return active !== null && libraryKinds.includes(active.kind);
  });
  const detailSkill = computed(() => detail()?.skill ?? skillById(skills(), detailId()));
  const deleteItems = computed(() => {
    const ids = deleteIds();
    return ids === null ? [] : skills().filter((skill) => ids.includes(skill.id));
  });

  let detailSequence = 0;

  function fail(prefix: MessageKey, error: unknown): void {
    store.notify("error", `${text(prefix)}: ${store.errorText(toAppError(error))}`);
  }

  /** Queues a library operation; `false` when the backend refused it. */
  function run(prefix: MessageKey, start: () => string): boolean {
    try {
      pendingId.set(start());
      return true;
    } catch (error) {
      fail(prefix, error);
      return false;
    }
  }

  // Selection

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
    for (const id of filteredIds()) {
      if (checked) {
        next.add(id);
      } else {
        next.delete(id);
      }
    }
    selectedIds.set(next);
  }

  function clearSelection(): void {
    selectedIds.set(new Set());
  }

  // Detail

  async function loadDetail(id: string): Promise<void> {
    const request = ++detailSequence;
    detailLoading.set(true);
    detailError.set(null);
    try {
      const result = await api.skillDetail(id);
      if (request === detailSequence) {
        detail.set(result);
      }
    } catch (error) {
      if (request === detailSequence) {
        detailError.set(toAppError(error));
      }
    } finally {
      if (request === detailSequence) {
        detailLoading.set(false);
      }
    }
  }

  function openDetail(skill: InstalledSkill): void {
    detail.set(null);
    detailError.set(null);
    detailId.set(skill.id);
  }

  function closeDetail(): void {
    detailSequence += 1;
    detailId.set(null);
  }

  /** Drops what the detail dialog showed, once it is gone. */
  function detailClosed(): void {
    if (detailId() === null) {
      detail.set(null);
      detailError.set(null);
      detailLoading.set(false);
    }
  }

  function reloadDetail(): void {
    const id = detailId();
    if (id !== null) {
      void loadDetail(id);
    }
  }

  // Rename

  function openRename(skill: InstalledSkill): void {
    closeDetail();
    renameError.set(null);
    renameValue.set(skill.display_name);
    renameTarget.set(skill);
  }

  function closeRename(): void {
    renameTarget.set(null);
    renameError.set(null);
  }

  function editRename(value: string): void {
    renameValue.set(value);
    renameError.set(null);
  }

  function submitRename(): void {
    const target = renameTarget();
    if (target === null) {
      return;
    }
    const name = renameValue().trim();
    if (name === "") {
      renameError.set(text("library.rename.invalid"));
      return;
    }
    if (name === target.display_name) {
      closeRename();
      return;
    }
    try {
      pendingId.set(api.renameSkill(target.id, name));
      closeRename();
    } catch (error) {
      renameError.set(store.errorText(toAppError(error)));
    }
  }

  // Delete

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
    const items = deleteItems();
    closeDelete();
    if (items.length === 0) {
      return;
    }
    const confirmModified = items.some((skill) => skill.local_state === "modified");
    run("library.delete.title", () =>
      api.deleteSkills(
        items.map((skill) => skill.id),
        confirmModified,
      ),
    );
  }

  // Update

  function startUpdate(skill: InstalledSkill, policy: UpdateConflictPolicy): void {
    run("library.update", () => api.applyUpdate(skill.id, policy));
  }

  function requestUpdate(skill: InstalledSkill): void {
    if (busy()) {
      return;
    }
    closeDetail();
    if (skill.local_state === "modified") {
      overwriteTarget.set(skill);
      return;
    }
    startUpdate(skill, "reject");
  }

  function closeOverwrite(): void {
    overwriteTarget.set(null);
  }

  function confirmOverwrite(): void {
    const target = overwriteTarget();
    closeOverwrite();
    if (target !== null) {
      startUpdate(target, "overwrite");
    }
  }

  function checkUpdates(ids: string[]): void {
    if (ids.length === 0 || busy()) {
      return;
    }
    run("library.checkAll", () => api.checkUpdates(ids));
  }

  function checkAll(): void {
    checkUpdates(
      skills()
        .filter((skill) => skill.source !== null)
        .map((skill) => skill.id),
    );
  }

  function checkOne(skill: InstalledSkill): void {
    closeDetail();
    checkUpdates([skill.id]);
  }

  // Results

  /** The skill an update refused to overwrite, when that is why it failed. */
  function locallyModifiedSkill(error: AppError): InstalledSkill | null {
    if (error.code !== "LOCAL_MODIFIED" || typeof error.details !== "object") {
      return null;
    }
    const id = (error.details as Record<string, unknown> | null)?.skillId;
    return typeof id === "string" ? skillById(skills(), id) : null;
  }

  function report(snapshot: OperationSnapshot): void {
    if (snapshot.state === "cancelled") {
      store.notify("info", text("library.notice.cancelled"));
      return;
    }
    if (snapshot.state === "failed") {
      if (snapshot.error === null) {
        store.notify("error", text("library.notice.failed"));
        return;
      }
      const error = toAppError(snapshot.error);
      const target = locallyModifiedSkill(error);
      if (target === null) {
        fail("library.notice.failed", error);
        return;
      }
      overwriteTarget.set(target);
      return;
    }
    switch (snapshot.kind) {
      case "check_updates": {
        const result = snapshot.result as CheckUpdatesReport | null;
        const failed = result?.failed.length ?? 0;
        const available = (result?.checked ?? []).filter(
          (item) => item.update_state === "available",
        ).length;
        const summary =
          available === 0
            ? text("library.notice.checkNone")
            : text("library.notice.checkDone", { count: available });
        store.notify(
          failed === 0 ? "success" : "warning",
          failed === 0
            ? summary
            : `${summary}${text("library.notice.checkFailed", { count: failed })}`,
        );
        return;
      }
      case "delete": {
        const result = snapshot.result as DeleteReport | null;
        const deleted = result?.deleted.length ?? 0;
        const failed = result?.failed.length ?? 0;
        clearSelection();
        store.notify(
          failed === 0 ? "success" : "warning",
          failed === 0
            ? text("library.notice.deleteDone", { count: deleted })
            : text("library.notice.deletePartial", { count: deleted, failed }),
        );
        return;
      }
      case "rename": {
        const result = snapshot.result as InstalledSkill | null;
        store.notify("success", text("library.notice.renamed", { name: result?.display_name ?? "" }));
        return;
      }
      case "apply_update": {
        const result = snapshot.result as InstalledSkill | null;
        store.notify("success", text("library.notice.updated", { name: result?.display_name ?? "" }));
        return;
      }
      default:
        return;
    }
  }

  effect(() => {
    const id = pendingId();
    if (id === null) {
      return;
    }
    const snapshot = store.operation(id);
    if (snapshot !== null && isTerminal(snapshot)) {
      pendingId.set(null);
      report(snapshot);
    }
  });

  // The detail follows the library: it reloads when the skills change and
  // closes when its skill is gone.
  effect(() => {
    const id = detailId();
    if (id === null) {
      return;
    }
    if (!skills().some((skill) => skill.id === id)) {
      closeDetail();
      return;
    }
    void loadDetail(id);
  });

  // Skills that left the library leave the selection too.
  effect(() => {
    const ids = new Set(skills().map((skill) => skill.id));
    const current = selectedIds();
    const kept = [...current].filter((id) => ids.has(id));
    if (kept.length !== current.size) {
      selectedIds.set(new Set(kept));
    }
  });

  return {
    query,
    filter,
    view,
    selectedIds,
    skills,
    filtered,
    counts,
    selected,
    allFilteredSelected,
    someFilteredSelected,
    busy,
    toggle,
    toggleAll,
    clearSelection,
    detailOpen: () => detailId() !== null,
    detail,
    detailSkill,
    detailLoading,
    detailError,
    openDetail,
    closeDetail,
    detailClosed,
    reloadDetail,
    renameTarget,
    renameValue,
    renameError,
    openRename,
    closeRename,
    editRename,
    submitRename,
    deleteItems,
    deleteOpen: () => deleteIds() !== null,
    requestDelete,
    closeDelete,
    confirmDelete,
    overwriteTarget,
    requestUpdate,
    closeOverwrite,
    confirmOverwrite,
    checkAll,
    checkOne,
  };
}
