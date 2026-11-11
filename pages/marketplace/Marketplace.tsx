import { List, View, windowSize } from "nsh";
import {
  Banner,
  Button,
  EmptyState,
  Label,
  LoadingState,
  Notices,
  Page,
  PageHeader,
  Segmented,
  Spinner,
  TextField,
  Toolbar,
} from "../../components/ui";
import type { MessageKey } from "../../i18n";
import { formatCount } from "../../lib/format";
import type { LeaderboardKind, MarketplaceSkill } from "../../lib/types";
import type { AppStore } from "../../store";
import { MarketCard } from "./components/MarketCard";
import { PreviewModal } from "./components/PreviewModal";
import { createMarketplaceController } from "./controller";
import { gridColumnsFor, sourceFilters, type SourceFilter } from "./model";

const kinds: LeaderboardKind[] = ["all_time", "trending", "hot"];

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

export function MarketplacePage(props: { store: AppStore }) {
  const text = props.store.text;
  const c = createMarketplaceController(props.store);

  return (
    <Page>
      <PageHeader title={text("marketplace.title")} subtitle={text("marketplace.subtitle")}>
        <TextField
          class="w-[300px]"
          icon="search"
          value={c.query()}
          placeholder={text("marketplace.searchPlaceholder")}
          clearLabel={text("common.clear")}
          onChange={c.editQuery}
          onSubmit={c.submitQuery}
          testID="marketplace-search"
        />
        <Button
          label={text("common.refresh")}
          icon="refresh"
          disabled={c.loading()}
          onPress={c.refresh}
          testID="marketplace-refresh"
        />
      </PageHeader>

      <Toolbar>
        <If
          when={!c.searching()}
          else={<Label tone="muted">{text("marketplace.searchResults")}</Label>}
        >
          {() => (
            <Segmented
              options={kinds}
              value={c.kind()}
              label={(option) => text(kindKeys[option])}
              onChange={c.selectKind}
              testID="marketplace-kind"
            />
          )}
        </If>
        <Segmented
          options={sourceFilters}
          value={c.sourceFilter()}
          label={(option) => text(sourceKeys[option])}
          onChange={(value) => c.sourceFilter.set(value)}
          testID="marketplace-source"
        />
        <View class="flex-1" />
        <If when={c.loading() && c.entries() !== null}>{() => <Spinner />}</If>
        <If when={c.entries() !== null}>
          {() => (
            <Label variant="caption" tone="muted">
              {text("marketplace.resultCount", { count: formatCount(c.filtered().length) })}
            </Label>
          )}
        </If>
      </Toolbar>

      <If when={c.loadError()}>
        {(error) => (
          <View class="pb-3">
            <Notices>
              <Banner
                tone="danger"
                message={`${text("marketplace.error.title")}: ${props.store.errorText(error())}`}
                testID="marketplace-error"
              >
                <Button label={text("common.retry")} variant="ghost" size="sm" onPress={c.refresh} />
              </Banner>
            </Notices>
          </View>
        )}
      </If>

      <If when={c.entries() === null && c.loading()}>
        {() => <LoadingState label={text("marketplace.loading")} />}
      </If>

      <If when={c.entries() !== null && c.filtered().length === 0}>
        {() => (
          <If
            when={(c.entries()?.length ?? 0) > 0}
            else={
              <EmptyState
                icon={c.searching() ? "searchX" : "store"}
                title={c.searching() ? text("marketplace.searchEmpty") : text("marketplace.empty")}
                hint={c.searching() ? undefined : text("marketplace.emptyHint")}
              />
            }
          >
            {() => (
              <EmptyState icon="searchX" title={text("marketplace.filteredEmpty")}>
                <Button
                  label={text("marketplace.clearFilter")}
                  onPress={() => c.sourceFilter.set("all")}
                />
              </EmptyState>
            )}
          </If>
        )}
      </If>

      <If when={c.filtered().length > 0}>
        {() => (
          <List<MarketplaceSkill>
            each={c.filtered()}
            numColumns={gridColumnsFor(windowSize().width)}
            estimatedItemSize={104}
            class="flex-1"
            contentClass="gap-3 px-6 pb-6"
            testID="marketplace-grid"
          >
            {(entry) => (
              <MarketCard
                entry={entry()}
                status={c.status(entry())}
                hasAvatar={c.hasAvatar(entry())}
                avatarUrl={c.avatarUrl(entry())}
                text={text}
                onAvatarError={() => c.avatarFailed(entry())}
                onPreview={(item) => void c.openPreview(item)}
                onInstall={c.install}
              />
            )}
          </List>
        )}
      </If>

      <PreviewModal store={props.store} controller={c} />
    </Page>
  );
}
