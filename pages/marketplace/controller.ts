import { cleanup, computed, signal, type ListRef, type ScrollEvent } from "nsh";
import * as api from "../../lib/api";
import { toAppError } from "../../lib/errors";
import type { AppError, LeaderboardKind, MarketplaceSkill, SkillPreview } from "../../lib/types";
import type { AppStore } from "../../store";
import {
  SEARCH_LIMIT,
  filterBySource,
  githubOwner,
  installedLineages,
  type EntryStatus,
  type SourceFilter,
} from "./model";

const SEARCH_DELAY_MS = 350;

/** How far the grid scrolls down before the back-to-top button shows, in points. */
const BACK_TO_TOP_THRESHOLD = 30;

export type MarketplaceController = ReturnType<typeof createMarketplaceController>;

/** The marketplace page's state: the ranking or search shown, and the preview open. */
export function createMarketplaceController(store: AppStore) {
  const kind = signal<LeaderboardKind>("all_time");
  const query = signal("");
  const appliedQuery = signal("");
  const sourceFilter = signal<SourceFilter>("all");
  const entries = signal<MarketplaceSkill[] | null>(null);
  const loading = signal(false);
  const loadError = signal<AppError | null>(null);

  const previewEntry = signal<MarketplaceSkill | null>(null);
  const previewOpen = signal(false);
  const preview = signal<SkillPreview | null>(null);
  const previewLoading = signal(false);
  const previewError = signal<AppError | null>(null);

  const avatarFailures = signal<ReadonlySet<string>>(new Set<string>());

  const grid = signal<ListRef | null>(null);
  const scrolled = signal(false);

  const installed = computed(() => installedLineages(store.skills()));
  const filtered = computed(() => filterBySource(entries() ?? [], sourceFilter()));
  const searching = computed(() => appliedQuery() !== "");

  let loadSequence = 0;
  let previewSequence = 0;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  function stopSearchTimer(): void {
    if (searchTimer !== null) {
      clearTimeout(searchTimer);
      searchTimer = null;
    }
  }

  async function load(force: boolean): Promise<void> {
    const request = ++loadSequence;
    loading.set(true);
    loadError.set(null);
    avatarFailures.set(new Set<string>());
    const term = appliedQuery();
    try {
      const result =
        term !== ""
          ? await api.marketSearch(term, SEARCH_LIMIT, force)
          : await api.marketLeaderboard(kind(), force);
      if (request === loadSequence) {
        entries.set(result);
      }
    } catch (error) {
      if (request === loadSequence) {
        loadError.set(toAppError(error));
      }
    } finally {
      if (request === loadSequence) {
        loading.set(false);
      }
    }
  }

  function applyQuery(term: string): void {
    stopSearchTimer();
    if (term === appliedQuery()) {
      return;
    }
    appliedQuery.set(term);
    void load(false);
  }

  /** The search field changed: searches once typing pauses, at once when emptied. */
  function editQuery(value: string): void {
    query.set(value);
    stopSearchTimer();
    if (value.trim() === "") {
      applyQuery("");
      return;
    }
    searchTimer = setTimeout(() => applyQuery(query().trim()), SEARCH_DELAY_MS);
  }

  function submitQuery(): void {
    applyQuery(query().trim());
  }

  function selectKind(next: LeaderboardKind): void {
    stopSearchTimer();
    kind.set(next);
    query.set("");
    appliedQuery.set("");
    entries.set(null);
    void load(false);
  }

  function refresh(): void {
    void load(true);
  }

  function status(entry: MarketplaceSkill): EntryStatus {
    const tracker = store.installs().find((item) => item.sourceKey === entry.lineage.source_key);
    if (tracker !== undefined) {
      return store.operation(tracker.operationId)?.state === "running" ? "installing" : "queued";
    }
    return installed().has(entry.lineage.source_key) ? "installed" : "available";
  }

  function install(entry: MarketplaceSkill): void {
    if (status(entry) === "available") {
      store.startInstall(entry.lineage, { name: entry.name, source: entry.source });
    }
  }

  /** Whether the entry's card holds the avatar's place: a GitHub owner, avatars turned on. */
  function hasAvatar(entry: MarketplaceSkill): boolean {
    return store.settings()?.show_market_avatars !== false && githubOwner(entry) !== null;
  }

  /** The GitHub avatar of the entry's owner, null when its image failed to load. */
  function avatarUrl(entry: MarketplaceSkill): string | null {
    const owner = githubOwner(entry);
    if (owner === null || avatarFailures().has(owner)) {
      return null;
    }
    return `https://github.com/${owner}.png?size=32`;
  }

  /** Drops an image that cannot load; the card keeps the avatar's place as a placeholder. */
  function avatarFailed(entry: MarketplaceSkill): void {
    const owner = githubOwner(entry);
    if (owner !== null && !avatarFailures().has(owner)) {
      avatarFailures.set(new Set([...avatarFailures(), owner]));
    }
  }

  /** Keeps the grid's ref, so the back-to-top button can scroll it. */
  function attachGrid(ref: ListRef): void {
    grid.set(ref);
  }

  /** Shows the back-to-top button once the grid has scrolled down far enough. */
  function trackScroll(event: ScrollEvent): void {
    const past = event.offset.y > BACK_TO_TOP_THRESHOLD;
    if (past !== scrolled()) {
      scrolled.set(past);
    }
  }

  function scrollToTop(): void {
    grid()?.scrollToOffset({ offset: 0, animated: true });
  }

  /** Opens the preview of an entry and downloads it: the only place a preview is fetched. */
  async function openPreview(entry: MarketplaceSkill): Promise<void> {
    previewEntry.set(entry);
    previewOpen.set(true);
    preview.set(null);
    previewError.set(null);
    const request = ++previewSequence;
    previewLoading.set(true);
    try {
      const result = await api.marketPreview(entry.lineage);
      if (request === previewSequence) {
        preview.set(result);
      }
    } catch (error) {
      if (request === previewSequence) {
        previewError.set(toAppError(error));
      }
    } finally {
      if (request === previewSequence) {
        previewLoading.set(false);
      }
    }
  }

  function closePreview(): void {
    previewSequence += 1;
    previewOpen.set(false);
  }

  /** Drops what the preview showed, once it is gone. */
  function previewClosed(): void {
    if (!previewOpen()) {
      previewEntry.set(null);
      preview.set(null);
      previewError.set(null);
      previewLoading.set(false);
    }
  }

  cleanup(stopSearchTimer);
  void load(false);

  return {
    kind,
    query,
    sourceFilter,
    entries,
    filtered,
    loading,
    loadError,
    searching,
    editQuery,
    submitQuery,
    selectKind,
    refresh,
    status,
    install,
    hasAvatar,
    avatarUrl,
    avatarFailed,
    scrolled,
    attachGrid,
    trackScroll,
    scrollToTop,
    previewEntry,
    previewOpen,
    preview,
    previewLoading,
    previewError,
    openPreview,
    closePreview,
    previewClosed,
  };
}
