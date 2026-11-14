import { computed, effect, signal } from "nsh";
import * as api from "../../lib/api";
import { toAppError } from "../../lib/errors";
import type { AppError, LocalImportReport, OperationSnapshot, UrlCandidate, UrlDiscovery } from "../../lib/types";
import type { AppStore } from "../../store";
import { summarizeImport } from "./import-model";

export type UrlImportController = ReturnType<typeof createUrlImportController>;

export function createUrlImportController(store: AppStore) {
  const query = signal("");
  const discovery = signal<UrlDiscovery | null>(null);
  const selectedIds = signal<ReadonlySet<string>>(new Set<string>());
  const loading = signal(false);
  const error = signal<AppError | null>(null);
  const pendingId = signal<string | null>(null);
  const report = signal<LocalImportReport | null>(null);
  const reportOpen = signal(false);
  const candidates = computed(() => discovery()?.candidates ?? []);
  const selected = computed(() => candidates().filter((candidate) => selectedIds().has(candidate.id)));
  const allSelected = computed(() => candidates().length > 0 && selected().length === candidates().length);
  const someSelected = computed(() => selected().length > 0 && !allSelected());
  const busy = computed(() => loading() || pendingId() !== null);
  const summary = computed(() => report() === null ? null : summarizeImport(report()!, store.text));
  let sequence = 0;
  let submitted: UrlCandidate[] = [];
  let settling = false;
  let invalidation: Promise<void> = Promise.resolve();

  function editQuery(value: string): void {
    if (pendingId() !== null) return;
    sequence += 1;
    query.set(value);
    discovery.set(null);
    selectedIds.set(new Set<string>());
    loading.set(false);
    error.set(null);
    report.set(null);
    invalidation = api.discardUrlImport();
    void invalidation.catch((failure) => store.notify("error", store.errorText(toAppError(failure))));
  }

  async function discover(): Promise<void> {
    if (pendingId() !== null || query().trim() === "") return;
    const request = ++sequence;
    const input = query().trim();
    discovery.set(null);
    selectedIds.set(new Set<string>());
    error.set(null);
    report.set(null);
    loading.set(true);
    try {
      await invalidation;
      if (request !== sequence) return;
      const result = await api.discoverUrlImport(input);
      if (request === sequence) discovery.set(result);
    } catch (failure) {
      if (request === sequence) error.set(toAppError(failure));
    } finally {
      if (request === sequence) loading.set(false);
    }
  }

  function toggle(id: string, checked: boolean): void {
    if (busy()) return;
    const next = new Set(selectedIds());
    if (checked) next.add(id); else next.delete(id);
    selectedIds.set(next);
  }

  function toggleAll(checked: boolean): void {
    if (busy()) return;
    selectedIds.set(new Set(checked ? candidates().map((candidate) => candidate.id) : []));
  }

  async function settle(snapshot: OperationSnapshot): Promise<void> {
    if (settling) return;
    settling = true;
    const result: LocalImportReport = snapshot.result as LocalImportReport | null ?? {
      items: submitted.map((candidate) => ({ candidate_id: candidate.id, name: candidate.name, path: candidate.path,
        status: snapshot.state === "cancelled" ? "cancelled" : "failed", skill: null, error: snapshot.error })),
    };
    report.set(result);
    selectedIds.set(new Set<string>());
    store.notify(result.items.some((item) => item.status === "failed") ? "warning"
      : result.items.some((item) => item.status === "cancelled") ? "info" : "success", summarizeImport(result, store.text));
    try {
      discovery.set(await api.urlImportSnapshot());
      await store.refreshSkills();
    } catch (failure) {
      store.notify("error", store.errorText(toAppError(failure)));
    } finally {
      submitted = [];
      pendingId.set(null);
      settling = false;
    }
  }

  effect(() => {
    const id = pendingId();
    if (id === null) return;
    const snapshot = store.operation(id) ?? api.operationSnapshot(id);
    if (snapshot !== null && snapshot.state !== "queued" && snapshot.state !== "running") void settle(snapshot);
  });

  effect(() => {
    const id = pendingId();
    if (id === null) return;
    const timer = setInterval(() => {
      const snapshot = api.operationSnapshot(id);
      if (snapshot !== null && snapshot.state !== "queued" && snapshot.state !== "running") void settle(snapshot);
    }, 500);
    return () => clearInterval(timer);
  });

  function importSelected(): void {
    const session = discovery();
    if (busy() || session === null || selected().length === 0) return;
    submitted = selected();
    report.set(null);
    try {
      pendingId.set(api.importUrlCandidates(session.session_id, submitted.map((candidate) => candidate.id)));
    } catch (failure) {
      submitted = [];
      store.notify("error", store.errorText(toAppError(failure)));
    }
  }

  function cancel(): void {
    const id = pendingId();
    if (id !== null) store.cancelOperation(id);
  }

  return { query, discovery, candidates, selectedIds, selected, allSelected, someSelected, loading,
    error, pendingId, report, reportOpen, busy, summary, editQuery, discover, toggle, toggleAll, importSelected, cancel };
}
