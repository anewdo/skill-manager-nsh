import { computed, signal } from "nsh";
import type { ImportCandidate } from "../../lib/types";
import type { AppStore } from "../../store";
import { parseRepositoryAddress, groupCandidates } from "./import-model";
import {
  mockAgentCandidates,
  mockFolderCandidates,
  mockRepositoryCandidates,
  mockZipCandidates,
} from "./mock-import";

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

  const pickedCandidates = signal<ImportCandidate[]>([]);
  const localSelectedIds = signal<ReadonlySet<string>>(new Set<string>());
  const localCandidates = computed(() => {
    const managed = new Set(store.skills().map((skill) => skill.display_name));
    return [...mockAgentCandidates(managed), ...pickedCandidates()];
  });
  const localSelected = computed(
    () => localCandidates().filter((candidate) => localSelectedIds().has(candidate.id)).length,
  );
  const localAllSelected = computed(() => everySelected(localCandidates(), localSelectedIds()));
  const localSomeSelected = computed(() => someSelected(localCandidates(), localSelectedIds()));
  const localGroups = computed(() => groupCandidates(localCandidates(), text));
  const collapsedGroups = signal<ReadonlySet<string>>(new Set<string>());

  const url = signal("");
  const urlError = signal<string | null>(null);
  const urlRepository = signal<string | null>(null);
  const urlCandidates = signal<ImportCandidate[] | null>(null);
  const urlSelectedIds = signal<ReadonlySet<string>>(new Set<string>());
  const urlSelected = computed(
    () => (urlCandidates() ?? []).filter((candidate) => urlSelectedIds().has(candidate.id)).length,
  );
  const urlAllSelected = computed(() =>
    everySelected(urlCandidates() ?? [], urlSelectedIds()),
  );
  const urlSomeSelected = computed(() =>
    someSelected(urlCandidates() ?? [], urlSelectedIds()),
  );

  function selectTab(next: MarketplaceTab): void {
    tab.set(next);
  }

  function addCandidates(candidates: ImportCandidate[]): void {
    const known = new Set(localCandidates().map((candidate) => candidate.id));
    const fresh = candidates.filter((candidate) => !known.has(candidate.id));
    if (fresh.length > 0) {
      pickedCandidates.set([...pickedCandidates(), ...fresh]);
    }
  }

  /** Scans the Agent directories again and starts the selection over. */
  function rescanLocal(): void {
    pickedCandidates.set([]);
    localSelectedIds.set(new Set<string>());
  }

  function chooseFolder(): void {
    addCandidates(mockFolderCandidates());
  }

  function chooseZip(): void {
    addCandidates(mockZipCandidates());
  }

  function toggleLocal(id: string, checked: boolean): void {
    localSelectedIds.set(
      checked
        ? addToSelection(localSelectedIds(), id)
        : removeFromSelection(localSelectedIds(), id),
    );
  }

  function toggleAllLocal(checked: boolean): void {
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

  function editUrl(value: string): void {
    url.set(value);
    urlError.set(null);
  }

  function discoverUrl(): void {
    const parsed = parseRepositoryAddress(url());
    if (parsed === null) {
      urlError.set(text("import.url.invalid"));
      urlRepository.set(null);
      urlCandidates.set(null);
      return;
    }
    urlError.set(null);
    urlRepository.set(parsed.repository);
    urlCandidates.set(mockRepositoryCandidates(parsed.repository));
    urlSelectedIds.set(new Set<string>());
  }

  function toggleUrl(id: string, checked: boolean): void {
    urlSelectedIds.set(
      checked ? addToSelection(urlSelectedIds(), id) : removeFromSelection(urlSelectedIds(), id),
    );
  }

  function toggleAllUrl(checked: boolean): void {
    urlSelectedIds.set(
      checked
        ? new Set((urlCandidates() ?? []).map((candidate) => candidate.id))
        : new Set<string>(),
    );
  }

  function importSelected(source: "local" | "url"): void {
    const count = source === "local" ? localSelected() : urlSelected();
    if (count === 0) {
      return;
    }
    store.notify("info", text("import.pending"));
  }

  return {
    tab,
    selectTab,
    localCandidates,
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
    toggleLocal,
    toggleAllLocal,
    url,
    urlError,
    urlRepository,
    urlCandidates,
    urlSelectedIds,
    urlSelected,
    urlAllSelected,
    urlSomeSelected,
    editUrl,
    discoverUrl,
    toggleUrl,
    toggleAllUrl,
    importSelected,
  };
}
