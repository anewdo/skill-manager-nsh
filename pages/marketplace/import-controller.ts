import { computed, effect, signal } from "nsh";
import * as api from "../../lib/api";
import { toAppError } from "../../lib/errors";
import type {
  ImportCandidate,
  LocalDiscovery,
  LocalImportReport,
  OperationSnapshot,
} from "../../lib/types";
import type { AppStore } from "../../store";
import { groupCandidates, localImportRows, summarizeImport } from "./import-model";

export type MarketplaceTab = "square" | "local" | "url";

export type ImportController = ReturnType<typeof createImportController>;

function everySelected(
  candidates: readonly ImportCandidate[],
  selected: ReadonlySet<string>,
): boolean {
  return candidates.length > 0 && candidates.every((candidate) => selected.has(candidate.id));
}

function someSelected(
  candidates: readonly ImportCandidate[],
  selected: ReadonlySet<string>,
): boolean {
  return (
    !everySelected(candidates, selected) &&
    candidates.some((candidate) => selected.has(candidate.id))
  );
}

function removeFromSelection(selected: ReadonlySet<string>, id: string): ReadonlySet<string> {
  const next = new Set(selected);
  next.delete(id);
  return next;
}

function addToSelection(selected: ReadonlySet<string>, id: string): ReadonlySet<string> {
  const next = new Set(selected);
  next.add(id);
  return next;
}

/** The page's tab and the discovery state of each import tab. */
export function createImportController(store: AppStore) {
  const text = store.text;
  const tab = signal<MarketplaceTab>("square");

  const localDiscovery = signal<LocalDiscovery>({ candidates: [], sources: [], issues: [] });
  const localSelectedIds = signal<ReadonlySet<string>>(new Set<string>());
  const localCandidates = computed(() => localDiscovery().candidates);
  const localLoading = signal(false);
  const localPendingId = signal<string | null>(null);
  const localBusy = computed(() => localLoading() || localPendingId() !== null);
  const localReport = signal<LocalImportReport | null>(null);
  const localReportOpen = signal(false);
  const localSummary = computed(() => {
    const report = localReport();
    return report === null ? null : summarizeImport(report, text);
  });
  const localHasZip = computed(() => localDiscovery().sources.some((source) => source.kind === "zip"));
  const localSelected = computed(
    () => localCandidates().filter((candidate) => localSelectedIds().has(candidate.id)).length,
  );
  const localAllSelected = computed(() => everySelected(localCandidates(), localSelectedIds()));
  const localSomeSelected = computed(() => someSelected(localCandidates(), localSelectedIds()));
  const localGroups = computed(() => groupCandidates(localCandidates(), text));
  const collapsedGroups = signal<ReadonlySet<string>>(new Set<string>());
  const localRows = computed(() =>
    localImportRows(localGroups(), collapsedGroups(), localDiscovery()),
  );

  function selectTab(next: MarketplaceTab): void {
    tab.set(next);
  }

  async function discoverLocal(
    work: () => Promise<LocalDiscovery | null>,
    reset = false,
  ): Promise<void> {
    if (localBusy()) return;
    localLoading.set(true);
    try {
      const result = await work();
      if (result === null) return;
      localDiscovery.set(result);
      const valid = new Set(result.candidates.map((candidate) => candidate.id));
      localSelectedIds.set(
        reset ? new Set<string>() : new Set([...localSelectedIds()].filter((id) => valid.has(id))),
      );
      localReport.set(null);
    } catch (error) {
      store.notify(
        "error",
        `${text("import.local.discoveryFailed")}: ${store.errorText(toAppError(error))}`,
      );
    } finally {
      localLoading.set(false);
    }
  }

  function chooseFolder(): void {
    void discoverLocal(() => api.chooseLocalImport("folder"));
  }

  function chooseZip(): void {
    void discoverLocal(() => api.chooseLocalImport("zip"));
  }

  function dropLocal(paths: string[]): void {
    void discoverLocal(() => api.discoverLocalImport(paths));
  }

  function rescanLocal(): void {
    void discoverLocal(api.rescanLocalImport, true);
  }

  function discardLocal(): void {
    void discoverLocal(api.discardLocalImport, true);
  }

  function toggleLocal(id: string, checked: boolean): void {
    if (localBusy()) return;
    localSelectedIds.set(
      checked
        ? addToSelection(localSelectedIds(), id)
        : removeFromSelection(localSelectedIds(), id),
    );
  }

  function toggleAllLocal(checked: boolean): void {
    if (localBusy()) return;
    localSelectedIds.set(
      checked ? new Set(localCandidates().map((candidate) => candidate.id)) : new Set<string>(),
    );
  }

  function toggleGroup(key: string): void {
    const next = new Set(collapsedGroups());
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    collapsedGroups.set(next);
  }

  let submitted: ImportCandidate[] = [];
  let settling = false;

  async function settleLocal(snapshot: OperationSnapshot): Promise<void> {
    if (settling) return;
    settling = true;
    const report: LocalImportReport = (snapshot.result as LocalImportReport | null) ?? {
      items: submitted.map((candidate) => ({
        candidate_id: candidate.id,
        name: candidate.name,
        path: candidate.path,
        status: snapshot.state === "cancelled" ? "cancelled" : "failed",
        skill: null,
        error: snapshot.error,
      })),
    };
    localReport.set(report);
    localSelectedIds.set(new Set<string>());
    store.notify(
      report.items.some((item) => item.status === "failed")
        ? "warning"
        : report.items.some((item) => item.status === "cancelled") ? "info" : "success",
      summarizeImport(report, text),
    );
    try {
      localDiscovery.set(await api.localImportSnapshot());
      await store.refreshSkills();
    } catch (error) {
      store.notify("error", store.errorText(toAppError(error)));
    } finally {
      submitted = [];
      localPendingId.set(null);
      settling = false;
    }
  }

  effect(() => {
    const id = localPendingId();
    if (id === null) return;
    // 同步快照也覆盖任务在订阅事件到达前完成的情况。
    const snapshot = store.operation(id) ?? api.operationSnapshot(id);
    if (snapshot !== null && snapshot.state !== "queued" && snapshot.state !== "running") {
      void settleLocal(snapshot);
    }
  });

  effect(() => {
    const id = localPendingId();
    if (id === null) return;
    const timer = setInterval(() => {
      const snapshot = api.operationSnapshot(id);
      if (snapshot !== null && snapshot.state !== "queued" && snapshot.state !== "running") {
        void settleLocal(snapshot);
      }
    }, 500);
    return () => clearInterval(timer);
  });

  function cancelLocal(): void {
    const id = localPendingId();
    if (id !== null) store.cancelOperation(id);
  }

  function importLocalSelected(): void {
    if (localBusy() || localSelected() === 0) return;
    submitted = localCandidates().filter((candidate) => localSelectedIds().has(candidate.id));
    try {
      localReport.set(null);
      localPendingId.set(api.importLocalCandidates(submitted.map((candidate) => candidate.id)));
    } catch (error) {
      submitted = [];
      store.notify(
        "error",
        `${text("import.local.submitFailed")}: ${store.errorText(toAppError(error))}`,
      );
    }
  }

  return {
    tab,
    selectTab,
    localCandidates,
    localDiscovery,
    localLoading,
    localBusy,
    localPendingId,
    localReport,
    localReportOpen,
    localSummary,
    localHasZip,
    localRows,
    localSelectedIds,
    localSelected,
    localAllSelected,
    localSomeSelected,
    localGroups,
    collapsedGroups,
    toggleGroup,
    rescanLocal,
    chooseFolder,
    chooseZip,
    dropLocal,
    discardLocal,
    cancelLocal,
    toggleLocal,
    toggleAllLocal,
    importLocalSelected,
  };
}
