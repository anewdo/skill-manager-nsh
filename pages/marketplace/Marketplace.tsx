import { List, View, windowSize } from "nsh";
import type { IconName } from "../../components/icon";
import {
  Banner,
  Button,
  EmptyState,
  FloatingButton,
  Label,
  LoadingState,
  Notices,
  Page,
  PageTabs,
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
import { LocalImportPanel } from "./components/LocalImportPanel";
import { PreviewModal } from "./components/PreviewModal";
import { UrlImportPanel } from "./components/UrlImportPanel";
import { createImportController, type MarketplaceTab } from "./import-controller";
import { createMarketplaceController } from "./controller";
import { gridColumnsFor, sourceFilters, type SourceFilter } from "./model";

const kinds: LeaderboardKind[] = ["all_time", "trending", "hot"];

const tabs: MarketplaceTab[] = ["square", "local", "url"];

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

const tabKeys: Record<MarketplaceTab, MessageKey> = {
  square: "marketplace.tab.square",
  local: "marketplace.tab.local",
  url: "marketplace.tab.url",
};

const tabIcons: Record<MarketplaceTab, IconName> = {
  square: "bookOpen",
  local: "upload",
  url: "github",
};

const subtitleKeys: Record<MarketplaceTab, MessageKey> = {
  square: "marketplace.subtitle.square",
  local: "marketplace.subtitle.local",
  url: "marketplace.subtitle.url",
};

export function MarketplacePage(props: { store: AppStore }) {
  const text = props.store.text;
  const c = createMarketplaceController(props.store);
  const imp = createImportController(props.store);

  return (
    <Page>
      <PageTabs
        options={tabs}
        value={imp.tab()}
        label={(tab) => text(tabKeys[tab])}
        icon={(tab) => tabIcons[tab]}
        onChange={imp.selectTab}
        class="pt-3"
        testID="marketplace-tab"
      />

      <View class="px-6 pt-3">
        <Label variant="body" tone="muted">
          {text(subtitleKeys[imp.tab()])}
        </Label>
      </View>

      <If when={imp.tab() === "square"}>
        {() => (
          <View class="flex-1 pt-2">
            <Toolbar>
              <TextField
                class="w-[320px]"
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
              <View class="flex-1" />
              <If when={c.entries() !== null}>
                {() => (
                  <Label variant="caption" tone="muted">
                    {text("marketplace.resultCount", { count: formatCount(c.filtered().length) })}
                  </Label>
                )}
              </If>
            </Toolbar>

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
                      <Button
                        label={text("common.retry")}
                        variant="ghost"
                        size="sm"
                        onPress={c.refresh}
                      />
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
                      title={
                        c.searching() ? text("marketplace.searchEmpty") : text("marketplace.empty")
                      }
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
                <View class="relative flex-1">
                  <List<MarketplaceSkill>
                    each={c.filtered()}
                    numColumns={gridColumnsFor(windowSize().width)}
                    estimatedItemSize={104}
                    class="flex-1"
                    contentClass="gap-3 px-6 pb-6"
                    ref={c.attachGrid}
                    onScroll={c.trackScroll}
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
                  <If when={c.scrolled()}>
                    {() => (
                      <FloatingButton
                        icon="arrowUp"
                        label={text("marketplace.backToTop")}
                        class="absolute right-6 bottom-6"
                        onPress={c.scrollToTop}
                        testID="marketplace-back-to-top"
                      />
                    )}
                  </If>
                </View>
              )}
            </If>
          </View>
        )}
      </If>

      <If when={imp.tab() === "local"}>
        {() => <LocalImportPanel store={props.store} controller={imp} />}
      </If>

      <If when={imp.tab() === "url"}>
        {() => <UrlImportPanel store={props.store} controller={imp} />}
      </If>

      <PreviewModal store={props.store} controller={c} />
    </Page>
  );
}
