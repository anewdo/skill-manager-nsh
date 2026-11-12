import { Scroll, View } from "nsh";
import { Banner, Button, Card, Label, LoadingState, Markdown, Modal } from "../../../components/ui";
import { skillBody } from "../../../lib/format";
import type { AppStore } from "../../../store";
import type { MarketplaceController } from "../controller";
import { InstallButton } from "./MarketCard";

/** The remote content of a Marketplace Entry, fetched because the user asked to see it. */
export function PreviewModal(props: { store: AppStore; controller: MarketplaceController }) {
  const text = props.store.text;
  const c = props.controller;
  return (
    <Modal
      open={c.previewOpen()}
      size="lg"
      title={c.previewEntry()?.name ?? text("marketplace.preview.title")}
      subtitle={c.previewEntry()?.source}
      closeLabel={text("common.close")}
      onClose={c.closePreview}
      onClosed={c.previewClosed}
      footer={
        <If when={c.previewEntry()}>
          {(entry) => (
            <InstallButton
              status={c.status(entry())}
              text={text}
              onPress={() => {
                c.install(entry());
                c.closePreview();
              }}
              testID="market-preview-install"
            />
          )}
        </If>
      }
    >
      <If when={c.previewLoading()}>
        {() => <LoadingState label={text("marketplace.preview.loading")} />}
      </If>
      <If when={c.previewError()}>
        {(error) => (
          <Banner
            tone="danger"
            message={`${text("marketplace.preview.error")}: ${props.store.errorText(error())}`}
          >
            <Button
              label={text("common.retry")}
              variant="ghost"
              size="sm"
              onPress={() => {
                const entry = c.previewEntry();
                if (entry !== null) {
                  void c.openPreview(entry);
                }
              }}
            />
          </Banner>
        )}
      </If>
      <If when={c.preview()}>
        {(result) => (
          <Scroll class="max-h-[480px]" contentClass="gap-3">
            <Label tone="muted">
              {result().description || text("marketplace.preview.noDescription")}
            </Label>
            <Card class="p-4">
              <Markdown source={skillBody(result().skill_md)} />
            </Card>
          </Scroll>
        )}
      </If>
    </Modal>
  );
}
