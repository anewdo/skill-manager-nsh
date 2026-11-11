import { cleanup, computed, effect, signal, windowSize } from "nsh";
import { Scroll, Text, View } from "nsh";
import { Button, Dialog, Input, Pagination, Popover, Tab, Tabs, TextView } from "nsh/base";
import * as api from "../../lib/api";
import { Icon } from "../../components/icon";
import { ActionButton, Badge, Banner, IconButton, Spinner } from "../../components/ui";
import { describeError, toAppError } from "../../lib/errors";
import { translate, type MessageKey, type MessageParams } from "../../i18n";
import type { AppStore, InstallTracker } from "../../store";
import type {
  AppError,
  LeaderboardKind,
  MarketplaceSkill,
  OperationProgress,
  OperationSnapshot,
  SkillPreview,
} from "../../lib/types";
import {
  PAGE_SIZE,
  SEARCH_LIMIT,
  chunk,
  clampPage,
  filterBySource,
  formatCount,
  installedLineages,
  pageCount,
  pageRange,
  gridColumnsFor,
  paginate,
  skillBody,
  type SourceFilter,
} from "./model";

const kinds: LeaderboardKind[] = ["all_time", "trending", "hot"];
const sourceFilters: SourceFilter[] = ["all", "github", "well_known"];

const kindKeys: Record<LeaderboardKind, MessageKey> = {
  all_time: "marketplace.leaderboard.all_time",
  trending: "marketplace.leaderboard.trending",
  hot: "marketplace.leaderboard.hot",
};

const sourceKeys: Record<SourceFilter, MessageKey> = {
  all: "marketplace.sourceFilter.all",
  github: "marketplace.sourceFilter.github",
  well_known: "marketplace.sourceFilter.well_known",
};

const leaderboardTabClass =
  "group rounded-(--radius) px-3 py-1.5 active:bg-accent selected:bg-secondary";
const leaderboardTabTextClass =
  "text-sm text-muted-foreground group-selected:text-foreground";
const filterTabClass =
  "group rounded-(--radius) px-2.5 py-1 active:bg-accent selected:bg-secondary";
const filterTabTextClass = "text-xs text-muted-foreground group-selected:text-foreground";
const paginationItemClass =
  "group rounded-(--radius) px-2 py-1 text-xs text-muted-foreground active:bg-accent selected:bg-secondary selected:text-foreground disabled:opacity-40";

function fillerCells(count: number, columns: number): number[] {
  const cells: number[] = [];
  for (let index = count; index < columns; index += 1) {
    cells.push(index);
  }
  return cells;
}

function progressPercent(progress: OperationProgress | null): number | undefined {
  if (!progress || progress.current === undefined || progress.total === undefined) {
    return undefined;
  }
  if (progress.total <= 0) {
    return undefined;
  }
  const percent = (progress.current / progress.total) * 100;
  return Math.min(100, Math.max(0, percent));
}

export function MarketplacePage(props: { store: AppStore }) {
  const text = (key: MessageKey, params?: MessageParams): string =>
    translate(props.store.settings()?.language ?? "zh_cn")(key, params);

  const kind = signal<LeaderboardKind>("all_time");
  const query = signal("");
  const appliedQuery = signal("");
  const sourceFilter = signal<SourceFilter>("all");
  const page = signal(1);
  const dataset = signal<MarketplaceSkill[] | null>(null);
  const loading = signal(false);
  const loadError = signal<AppError | null>(null);
  const selected = signal<MarketplaceSkill | null>(null);
  const previewOpen = signal(false);
  const preview = signal<SkillPreview | null>(null);
  const previewLoading = signal(false);
  const previewError = signal<AppError | null>(null);

  const installed = computed(() => installedLineages(props.store.installations()));
  const filtered = computed(() => filterBySource(dataset() ?? [], sourceFilter()));
  const totalPages = computed(() => pageCount(filtered().length));
  const currentPage = computed(() => clampPage(page(), totalPages()));
  const columns = computed(() => gridColumnsFor(windowSize().width));
  const rows = computed(() => chunk(paginate(filtered(), currentPage()), columns()));
  const hasSkills = computed(() => (dataset()?.length ?? 0) > 0);
  const rangeSummary = computed(() => {
    const range = pageRange(currentPage(), filtered().length);
    if (!range) {
      return text("marketplace.rangeEmpty");
    }
    return text("marketplace.range", {
      start: range.start,
      end: range.end,
      total: filtered().length,
    });
  });

  let loadSequence = 0;
  let previewSequence = 0;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  function errorText(error: AppError): string {
    return describeError(error, translate(props.store.settings()?.language ?? "zh_cn"));
  }

  async function load(force: boolean): Promise<void> {
    const request = ++loadSequence;
    loading.set(true);
    loadError.set(null);
    const term = appliedQuery();
    try {
      const result =
        term !== ""
          ? await api.marketSearch(term, SEARCH_LIMIT, force)
          : await api.marketLeaderboard(kind(), force);
      if (request !== loadSequence) {
        return;
      }
      dataset.set(result);
    } catch (error) {
      if (request !== loadSequence) {
        return;
      }
      loadError.set(toAppError(error));
    } finally {
      if (request === loadSequence) {
        loading.set(false);
      }
    }
  }

  function selectKind(next: LeaderboardKind): void {
    if (next === kind()) {
      return;
    }
    kind.set(next);
    query.set("");
    appliedQuery.set("");
    page.set(1);
    dataset.set(null);
    closePreview();
    void load(false);
  }

  function selectSource(next: SourceFilter): void {
    if (next === sourceFilter()) {
      return;
    }
    sourceFilter.set(next);
    page.set(1);
  }

  function submitSearch(): void {
    if (searchTimer !== null) {
      clearTimeout(searchTimer);
      searchTimer = null;
    }
    const term = query().trim();
    if (term === appliedQuery()) {
      return;
    }
    appliedQuery.set(term);
    page.set(1);
    void load(false);
  }

  function scheduleSearch(): void {
    if (searchTimer !== null) {
      clearTimeout(searchTimer);
    }
    searchTimer = setTimeout(() => {
      searchTimer = null;
      submitSearch();
    }, 350);
  }

  function clearSearch(): void {
    query.set("");
    if (appliedQuery() === "") {
      return;
    }
    appliedQuery.set("");
    page.set(1);
    void load(false);
  }

  async function openPreview(entry: MarketplaceSkill): Promise<void> {
    selected.set(entry);
    previewOpen.set(true);
    preview.set(null);
    previewError.set(null);
    const request = ++previewSequence;
    previewLoading.set(true);
    try {
      const result = await api.marketPreview(entry.lineage);
      if (request !== previewSequence) {
        return;
      }
      preview.set(result);
    } catch (error) {
      if (request !== previewSequence) {
        return;
      }
      previewError.set(toAppError(error));
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

  function finishClosePreview(): void {
    if (previewOpen()) {
      return;
    }
    selected.set(null);
    preview.set(null);
    previewError.set(null);
    previewLoading.set(false);
  }

  function installTracker(entry: MarketplaceSkill): InstallTracker | null {
    return (
      props.store
        .installs()
        .find((tracker) => tracker.sourceKey === entry.lineage.source_key) ?? null
    );
  }

  function installSnapshot(entry: MarketplaceSkill): OperationSnapshot | null {
    const tracker = installTracker(entry);
    return tracker === null ? null : props.store.operation(tracker.operationId);
  }

  function isEntryInstalling(entry: MarketplaceSkill): boolean {
    return installTracker(entry) !== null;
  }

  const queueOpen = signal(false);
  const installQueue = props.store.installQueue;
  const installSpeeds = props.store.installSpeeds;
  const hasFailedInstalls = computed(() =>
    installQueue().some((snapshot) => snapshot.state === "failed"),
  );

  effect(() => {
    if (installQueue().length === 0) {
      queueOpen.set(false);
    }
  });

  function phaseLabel(phase: string): string {
    switch (phase) {
      case "downloading":
        return text("marketplace.queue.downloading");
      case "validating":
        return text("marketplace.queue.validating");
      case "installing":
        return text("marketplace.queue.installing");
      case "renaming":
        return text("marketplace.queue.renaming");
      case "deleting":
        return text("marketplace.queue.deleting");
      case "checking":
        return text("marketplace.queue.checking");
      default:
        return phase;
    }
  }

  function formatBytes(bytes: number): string {
    const units = ["B", "KB", "MB", "GB"];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return unit === 0 ? `${Math.round(value)} B` : `${value.toFixed(1)} ${units[unit]}`;
  }

  /** The second line of a queue row: transfer numbers while downloading, else the phase. */
  function queueRowDetail(snapshot: OperationSnapshot): string {
    if (snapshot.state === "queued") {
      return text("marketplace.queue.waiting");
    }
    if (snapshot.cancel_requested) {
      return text("marketplace.queue.cancelling");
    }
    if (snapshot.state === "failed") {
      return snapshot.error === null
        ? text("marketplace.queue.failed")
        : errorText(snapshot.error);
    }
    const progress = snapshot.progress;
    if (progress === null) {
      return text("marketplace.queue.downloading");
    }
    const parts: string[] = [];
    const percent = progressPercent(progress);
    if (percent !== undefined) {
      parts.push(`${Math.round(percent)}%`);
    }
    if (progress.phase === "downloading") {
      const speed = installSpeeds()[snapshot.id];
      if (speed !== undefined && speed > 0) {
        parts.push(`${formatBytes(speed)}/s`);
      }
      if (progress.bytes !== undefined) {
        const total =
          progress.bytes_total === undefined
            ? text("marketplace.queue.unknownTotal")
            : formatBytes(progress.bytes_total);
        parts.push(`${formatBytes(progress.bytes)} / ${total}`);
      }
    } else {
      parts.push(phaseLabel(progress.phase));
    }
    return parts.length === 0 ? phaseLabel(progress.phase) : parts.join(" · ");
  }

  function renderQueueRow(snapshot: OperationSnapshot) {
    const failed = snapshot.state === "failed";
    const cancellable =
      snapshot.state === "queued" || snapshot.progress?.cancellable === true;
    return (
      <View class="flex-row items-center gap-2 rounded-(--radius) px-2 py-1.5 hover:bg-accent">
        <View class="flex-1 gap-0.5">
          <View class="flex-row items-baseline gap-1.5">
            <Text class="text-sm font-medium text-foreground truncate">
              {snapshot.label?.name ?? text("marketplace.queue.unnamed")}
            </Text>
            <If when={snapshot.label !== undefined && snapshot.label.source !== ""}>
              {() => (
                <Text class="flex-1 text-xs text-muted-foreground truncate">
                  {snapshot.label?.source ?? ""}
                </Text>
              )}
            </If>
          </View>
          <Text
            class={`text-xs truncate ${failed ? "text-destructive" : "text-muted-foreground"}`}
          >
            {queueRowDetail(snapshot)}
          </Text>
        </View>
        <If when={failed}>
          {() => (
            <IconButton
              icon="close"
              label={text("marketplace.queue.dismiss")}
              onPress={() => props.store.dismissInstall(snapshot.id)}
            />
          )}
        </If>
        <If when={!failed && cancellable && !snapshot.cancel_requested}>
          {() => (
            <IconButton
              icon="close"
              label={text("marketplace.queue.cancel")}
              onPress={() => props.store.cancelInstall(snapshot.id)}
            />
          )}
        </If>
        <If when={!failed && snapshot.cancel_requested}>
          {() => <Spinner size={14} />}
        </If>
      </View>
    );
  }

  /** The button label: the download percentage while transferring, then the phase. */
  function installButtonLabel(entry: MarketplaceSkill): string {
    if (installTracker(entry) === null) {
      return installed().has(entry.lineage.source_key)
        ? text("marketplace.card.installed")
        : text("marketplace.card.install");
    }
    const snapshot = installSnapshot(entry);
    if (snapshot === null || snapshot.state === "queued") {
      return text("marketplace.card.installQueued");
    }
    return text("marketplace.card.installing");
  }

  cleanup(() => {
    if (searchTimer !== null) {
      clearTimeout(searchTimer);
      searchTimer = null;
    }
  });

  void load(false);
  void props.store.refreshInstallations();

  function renderCard(entry: MarketplaceSkill) {
    return (
      <View class="flex-1 gap-2 rounded-(--radius) border border-border bg-card p-4">
        <View class="flex-row items-start gap-2">
          <Text class="flex-1 text-base font-semibold text-foreground">{entry.name}</Text>
          <If when={installed().has(entry.lineage.source_key)}>
            {() => <Badge label={text("marketplace.card.installed")} />}
          </If>
        </View>
        <Text class="text-xs text-muted-foreground">{entry.source}</Text>
        <Text class="text-xs text-muted-foreground">
          {text("marketplace.card.installs", { count: formatCount(entry.installs) })}
        </Text>
        <View class="flex-row gap-2 pt-2">
          <ActionButton
            label={text("marketplace.card.preview")}
            variant="secondary"
            onPress={() => void openPreview(entry)}
          />
          <ActionButton
            label={installButtonLabel(entry)}
            variant="primary"
            busy={isEntryInstalling(entry)}
            disabled={installed().has(entry.lineage.source_key) || isEntryInstalling(entry)}
            onPress={() =>
              props.store.startInstall(entry.lineage, {
                name: entry.name,
                source: entry.source,
              })
            }
          />
        </View>
      </View>
    );
  }

  function renderEmptyState() {
    return (
      <View class="flex-1 items-center justify-center gap-2 p-12">
        <If when={hasSkills()}>
          {() => (
            <View class="items-center gap-2">
              <Text class="text-base font-medium text-foreground">
                {text("marketplace.filteredEmpty")}
              </Text>
              <ActionButton
                label={text("marketplace.clearFilter")}
                variant="secondary"
                onPress={() => selectSource("all")}
              />
            </View>
          )}
        </If>
        <If when={!hasSkills()}>
          {() => (
            <View class="items-center gap-2">
              <Text class="text-base font-medium text-foreground">
                {appliedQuery() !== "" ? text("marketplace.searchEmpty") : text("marketplace.empty")}
              </Text>
              <Text class="text-sm text-muted-foreground">{text("marketplace.emptyHint")}</Text>
            </View>
          )}
        </If>
      </View>
    );
  }

  return (
    <View class="flex-1 bg-background">
      <View class="gap-3 border-b border-border bg-card px-6 py-4">
        <View class="flex-row items-center gap-4">
          <Tabs class="flex-row gap-1">
            <Each of={kinds}>
              {(item) => (
                <Tab
                  selected={item() === kind()}
                  onPress={() => selectKind(item())}
                  class={leaderboardTabClass}
                >
                  <Text class={leaderboardTabTextClass}>{text(kindKeys[item()])}</Text>
                </Tab>
              )}
            </Each>
          </Tabs>
          <View class="flex-1 flex-row items-center gap-2">
            <Input
              class="flex-1 rounded-(--radius) border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
              value={query()}
              placeholder={text("marketplace.searchPlaceholder")}
              submitBehavior="submit"
              onChange={(event) => {
                query.set(event.value);
                scheduleSearch();
              }}
              onSubmit={submitSearch}
              testID="marketplace-search"
            />
            <If when={query() !== ""}>
              {() => (
                <ActionButton
                  label={text("common.clear")}
                  variant="ghost"
                  onPress={clearSearch}
                />
              )}
            </If>
          </View>
          <ActionButton
            label={text("common.refresh")}
            variant="secondary"
            disabled={loading()}
            onPress={() => void load(true)}
          />
        </View>
        <View class="flex-row items-center gap-3">
          <Tabs class="flex-row gap-1">
            <Each of={sourceFilters}>
              {(item) => (
                <Tab
                  selected={item() === sourceFilter()}
                  onPress={() => selectSource(item())}
                  class={filterTabClass}
                >
                  <Text class={filterTabTextClass}>{text(sourceKeys[item()])}</Text>
                </Tab>
              )}
            </Each>
          </Tabs>
          <View class="flex-1" />
          <If when={loading()}>
            {() => (
              <Text class="text-xs text-muted-foreground">
                {appliedQuery() !== ""
                  ? text("marketplace.searching")
                  : text("marketplace.loading")}
              </Text>
            )}
          </If>
          <If when={!loading() && dataset() !== null}>
            {() => (
              <Text class="text-xs text-muted-foreground">
                {text("marketplace.resultCount", { count: formatCount(filtered().length) })}
              </Text>
            )}
          </If>
        </View>
      </View>

      <View class="gap-2 px-6 pt-4">
        <If when={props.store.installationsError()}>
          {(error) => <Banner tone="warning" message={errorText(error())} />}
        </If>
        <If when={loadError()}>
          {(error) => (
            <Banner
              tone="error"
              message={`${text("marketplace.error.title")}: ${errorText(error())}`}
              actionLabel={text("common.retry")}
              onAction={() => void load(true)}
            />
          )}
        </If>
      </View>

      <If when={dataset() === null && loading()}>
        {() => (
          <View class="flex-1 items-center justify-center">
            <Text class="text-sm text-muted-foreground">{text("marketplace.loading")}</Text>
          </View>
        )}
      </If>

      <If when={dataset() !== null && filtered().length === 0}>
        {() => renderEmptyState()}
      </If>

      <If when={dataset() !== null && filtered().length > 0}>
        {() => (
          <Scroll class="flex-1">
            <View class="gap-4 px-6 pt-6 pb-24">
              <Each of={rows()} key="index">
                {(row) => (
                  <View class="flex-row items-stretch gap-4">
                    <Each of={row()}>
                      {(entry) => renderCard(entry())}
                    </Each>
                    <Each of={fillerCells(row().length, columns())}>
                      {() => <View class="flex-1" />}
                    </Each>
                  </View>
                )}
              </Each>
            </View>
          </Scroll>
        )}
      </If>

      <If when={dataset() !== null && filtered().length > 0}>
        {() => (
          <View class="flex-row items-center justify-between border-t border-border bg-card px-6 py-3">
            <Text class="text-xs text-muted-foreground">{rangeSummary()}</Text>
            <If when={totalPages() > 1}>
              {() => (
                <Pagination
                  page={currentPage()}
                  totalPages={totalPages()}
                  onChange={(event) => page.set(event.page)}
                  accessibilityLabel={text("marketplace.pagination.label")}
                  class="flex-row items-center gap-1"
                  itemClass={paginationItemClass}
                />
              )}
            </If>
          </View>
        )}
      </If>

      <Dialog
        open={previewOpen()}
        animation="fade"
        class="m-auto w-[720px] max-h-[640px] gap-4 rounded-(--radius) border border-border bg-popover p-6"
        backdropClass="bg-black/40"
        onClose={finishClosePreview}
        onOpenChange={(event) => {
          if (!event.open) {
            closePreview();
          }
        }}
      >
        <If when={selected()}>
          {(entry) => (
            <View class="gap-4">
              <View class="flex-row items-start gap-3">
                <View class="flex-1 gap-1">
                  <View class="flex-row items-center gap-2">
                    <Text class="text-lg font-semibold text-foreground">{entry().name}</Text>
                    <If when={installed().has(entry().lineage.source_key)}>
                      {() => <Badge label={text("marketplace.card.installed")} />}
                    </If>
                  </View>
                  <Text class="text-xs text-muted-foreground">{entry().source}</Text>
                </View>
                <ActionButton
                  label={text("common.close")}
                  variant="ghost"
                  onPress={closePreview}
                />
              </View>
              <Scroll class="max-h-[420px]" contentClass="gap-3">
                <If when={previewLoading()}>
                  {() => (
                    <Text class="text-sm text-muted-foreground">
                      {text("marketplace.preview.loading")}
                    </Text>
                  )}
                </If>
                <If when={previewError()}>
                  {(error) => (
                    <View class="gap-2">
                      <Text class="text-sm text-destructive">
                        {`${text("marketplace.preview.error")}: ${errorText(error())}`}
                      </Text>
                      <View class="flex-row">
                        <ActionButton
                          label={text("marketplace.preview.retry")}
                          variant="secondary"
                          onPress={() => void openPreview(entry())}
                        />
                      </View>
                    </View>
                  )}
                </If>
                <If when={preview()}>
                  {(result) => (
                    <View class="gap-3">
                      <Text class="text-sm text-foreground">
                        {result().description || text("marketplace.preview.noDescription")}
                      </Text>
                      <TextView
                        markdown={skillBody(result().skill_md)}
                        selectable
                        class="text-sm text-foreground"
                      />
                    </View>
                  )}
                </If>
              </Scroll>
              <View class="flex-row justify-end gap-2">
                <ActionButton
                  label={installButtonLabel(entry())}
                  variant="primary"
                  busy={isEntryInstalling(entry())}
                  disabled={
                    installed().has(entry().lineage.source_key) || isEntryInstalling(entry())
                  }
                  onPress={() => {
                    props.store.startInstall(entry().lineage, {
                      name: entry().name,
                      source: entry().source,
                    });
                    closePreview();
                  }}
                />
              </View>
            </View>
          )}
        </If>
      </Dialog>

      <If when={installQueue().length > 0}>
        {() => (
          <View style={{ position: "absolute", right: 24, bottom: 24 }}>
            <Popover
              open={queueOpen()}
              onOpenChange={(event) => queueOpen.set(event.open)}
              placement="top"
              align="end"
              offset={8}
              popupClass="w-[360px] gap-1 rounded-(--radius) border border-border bg-popover p-2"
              trigger={
                <Button
                  class="flex-row items-center gap-2 rounded-full border border-border bg-card px-3 py-2 active:bg-accent"
                  accessibilityLabel={text("marketplace.queue.button")}
                  testID="download-queue"
                >
                  <Icon name="download" color="foreground" />
                  <Text class="text-sm text-foreground">
                    {text("marketplace.queue.button")}
                  </Text>
                  <View class="rounded-full bg-secondary px-1.5 py-0.5">
                    <Text class="text-xs text-secondary-foreground">
                      {String(installQueue().length)}
                    </Text>
                  </View>
                </Button>
              }
            >
              <View class="gap-1">
                <View class="flex-row items-center justify-between px-2 py-1">
                  <Text class="text-sm font-semibold text-foreground">
                    {text("marketplace.queue.title")}
                  </Text>
                  <If when={hasFailedInstalls()}>
                    {() => (
                      <ActionButton
                        label={text("marketplace.queue.clear")}
                        variant="ghost"
                        onPress={() => props.store.clearFailedInstalls()}
                      />
                    )}
                  </If>
                </View>
                <Scroll class="max-h-[280px]" contentClass="gap-1">
                  <Each of={installQueue()} key={(item) => item.id}>
                    {(item) => renderQueueRow(item())}
                  </Each>
                </Scroll>
              </View>
            </Popover>
          </View>
        )}
      </If>
    </View>
  );
}
