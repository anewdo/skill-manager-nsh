import { Scroll, View } from "nsh";
import {
  Banner,
  Button,
  Card,
  Field,
  Label,
  LoadingState,
  Markdown,
  Modal,
} from "../../../components/ui";
import { formatBytes, skillBody } from "../../../lib/format";
import type { AppStore } from "../../../store";
import type { LibraryController } from "../controller";
import { localStateKeys, updateStateKeys } from "../labels";
import { isUpdateAvailable, sourceLabel } from "../model";

/** One Installed Skill in full: its states, where it is, and its SKILL.md. */
export function SkillDetailModal(props: { store: AppStore; controller: LibraryController }) {
  const text = props.store.text;
  const c = props.controller;
  return (
    <Modal
      open={c.detailOpen()}
      size="lg"
      title={c.detailSkill()?.display_name ?? text("library.detail.title")}
      subtitle={sourceLabel(c.detailSkill()?.source ?? null) || text("library.source.local")}
      closeLabel={text("common.close")}
      onClose={c.closeDetail}
      onClosed={c.detailClosed}
      footer={
        <If when={c.detailSkill()}>
          {(skill) => (
            <>
              <Button
                label={text("library.delete")}
                icon="trash"
                variant="danger"
                disabled={c.busy()}
                onPress={() => c.requestDelete([skill().id])}
              />
              <View class="flex-1" />
              <Button
                label={text("library.rename")}
                icon="rename"
                disabled={c.busy()}
                onPress={() => c.openRename(skill())}
              />
              <If when={skill().source !== null}>
                {() => (
                  <Button
                    label={text("library.checkOne")}
                    icon="refresh"
                    disabled={c.busy()}
                    onPress={() => c.checkOne(skill())}
                  />
                )}
              </If>
              <If when={isUpdateAvailable(skill())}>
                {() => (
                  <Button
                    label={text("library.update")}
                    icon="update"
                    variant="primary"
                    disabled={c.busy()}
                    onPress={() => c.requestUpdate(skill())}
                  />
                )}
              </If>
            </>
          )}
        </If>
      }
    >
      <If when={c.detailError()}>
        {(error) => (
          <Banner
            tone="danger"
            message={`${text("library.detail.error")}: ${props.store.errorText(error())}`}
          >
            <Button
              label={text("common.retry")}
              variant="ghost"
              size="sm"
              onPress={c.reloadDetail}
            />
          </Banner>
        )}
      </If>
      <If when={c.detail() === null && c.detailLoading()}>
        {() => <LoadingState label={text("library.detail.loading")} />}
      </If>
      <If when={c.detail()}>
        {(value) => (
          <Scroll class="max-h-[460px]" contentClass="gap-3">
            <Card class="gap-2 p-3">
              <Field label={text("library.detail.state")}>
                <Label class="flex-1">{text(localStateKeys[value().skill.local_state])}</Label>
              </Field>
              <Field label={text("library.detail.update")}>
                <Label class="flex-1">{text(updateStateKeys[value().skill.update_state])}</Label>
              </Field>
              <Field label={text("library.detail.path")}>
                <Label class="flex-1" selectable>
                  {value().local.path}
                </Label>
              </Field>
              <If when={value().skill.source}>
                {(source) => (
                  <Field label={text("library.detail.source")}>
                    <Label class="flex-1" selectable>
                      {source().source_key}
                    </Label>
                  </Field>
                )}
              </If>
              <Field label={text("library.detail.files")}>
                <Label class="flex-1">
                  {value().local.file_count === null
                    ? "—"
                    : `${value().local.file_count} · ${formatBytes(value().local.total_bytes)}`}
                </Label>
              </Field>
              <If when={value().local.metadata?.description}>
                {(description) => (
                  <Field label={text("library.detail.description")}>
                    <Label class="flex-1">{description()}</Label>
                  </Field>
                )}
              </If>
            </Card>
            <If when={value().local.state === "missing"}>
              {() => <Banner tone="warning" message={text("library.detail.missingFilesHint")} />}
            </If>
            <If when={value().local.skill_md_error}>
              {(error) => (
                <Banner
                  tone="danger"
                  message={`${text("library.detail.skillMdError")}: ${props.store.errorText(error())}`}
                />
              )}
            </If>
            <If when={value().local.skill_md}>
              {(markdown) => (
                <Card class="p-4">
                  <Markdown source={skillBody(markdown())} />
                </Card>
              )}
            </If>
            <If
              when={
                value().local.skill_md === null &&
                value().local.skill_md_error === null &&
                value().local.state !== "missing"
              }
            >
              {() => <Label tone="muted">{text("library.detail.noSkillMd")}</Label>}
            </If>
          </Scroll>
        )}
      </If>
    </Modal>
  );
}
