import { Scroll, Text, View } from "nsh";
import { Dialog, TextView } from "nsh/base";
import trashIcon from "../../../assets/icons/trash.svg";
import { ActionButton, Banner, IconButton } from "../../../components/ui";
import type { AppError, Installation, InstallationDetail } from "../../../types";
import {
  localStateKeys,
  updateStateKeys,
  type TextFn,
} from "../labels";
import { formatBytes, isUpdateAvailable, sourceLabel } from "../model";
import { skillBody } from "../../marketplace/model";

export interface SkillDetailDialogProps {
  open: () => boolean;
  installation: () => Installation | null;
  detail: () => InstallationDetail | null;
  loading: () => boolean;
  error: () => AppError | null;
  busy: () => boolean;
  text: TextFn;
  errorText: (error: AppError) => string;
  onClose: () => void;
  onRefresh: () => void;
  onCheck: (installation: Installation) => void;
  onUpdate: (installation: Installation) => void;
  onRename: (installation: Installation) => void;
  onDelete: (installation: Installation) => void;
}

export function SkillDetailDialog(props: SkillDetailDialogProps) {
  const text = props.text;
  return (
    <Dialog
      open={props.open()}
      animation="fade"
      class="m-auto w-[720px] max-h-[680px] gap-4 rounded-(--radius) border border-border bg-popover p-6"
      backdropClass="bg-black/40"
      onOpenChange={(event) => {
        if (!event.open) {
          props.onClose();
        }
      }}
    >
      <View class="gap-4">
        <View class="flex-row items-start gap-3">
          <View class="flex-1 gap-1">
            <Text class="text-lg font-semibold text-foreground">
              {props.installation()?.display_name ?? text("myskills.detail.title")}
            </Text>
            <Text class="text-xs text-muted-foreground">
              {sourceLabel(props.installation()?.source ?? null)}
            </Text>
          </View>
          <ActionButton label={text("common.close")} variant="ghost" onPress={props.onClose} />
        </View>

        <If when={props.loading()}>
          {() => (
            <Text class="text-sm text-muted-foreground">{text("myskills.detail.loading")}</Text>
          )}
        </If>
        <If when={props.error()}>
          {(error) => (
            <Banner
              tone="error"
              message={`${text("myskills.detail.error")}: ${props.errorText(error())}`}
              actionLabel={text("myskills.detail.refresh")}
              onAction={props.onRefresh}
            />
          )}
        </If>

        <If when={props.detail()}>
          {(value) => (
            <Scroll class="max-h-[440px]" contentClass="gap-3">
              <If when={props.installation()}>
                {(item) => (
                  <View class="gap-1 rounded-(--radius) border border-border bg-card px-3 py-2">
                    <View class="flex-row gap-2">
                      <Text class="w-24 text-xs text-muted-foreground">
                        {text("myskills.detail.state")}
                      </Text>
                      <Text class="flex-1 text-xs text-foreground">
                        {text(localStateKeys[item().local_state])}
                      </Text>
                    </View>
                    <View class="flex-row gap-2">
                      <Text class="w-24 text-xs text-muted-foreground">
                        {text("myskills.detail.update")}
                      </Text>
                      <Text class="flex-1 text-xs text-foreground">
                        {text(updateStateKeys[item().update_state])}
                      </Text>
                    </View>
                    <View class="flex-row gap-2">
                      <Text class="w-24 text-xs text-muted-foreground">
                        {text("myskills.detail.path")}
                      </Text>
                      <Text class="flex-1 text-xs text-foreground">{item().root_relpath}</Text>
                    </View>
                    <View class="flex-row gap-2">
                      <Text class="w-24 text-xs text-muted-foreground">
                        {text("myskills.detail.source")}
                      </Text>
                      <Text class="flex-1 text-xs text-foreground">
                        {item().source?.source_key ?? ""}
                      </Text>
                    </View>
                  </View>
                )}
              </If>

              <View class="gap-1 rounded-(--radius) border border-border bg-card px-3 py-2">
                <View class="flex-row gap-2">
                  <Text class="w-24 text-xs text-muted-foreground">
                    {text("myskills.detail.files")}
                  </Text>
                  <Text class="flex-1 text-xs text-foreground">
                    {value().local.file_count === null ? "—" : String(value().local.file_count)}
                  </Text>
                </View>
                <View class="flex-row gap-2">
                  <Text class="w-24 text-xs text-muted-foreground">
                    {text("myskills.detail.size")}
                  </Text>
                  <Text class="flex-1 text-xs text-foreground">
                    {formatBytes(value().local.total_bytes)}
                  </Text>
                </View>
                <If when={value().local.metadata}>
                  {(metadata) => (
                    <View class="gap-1">
                      <Text class="text-xs text-muted-foreground">
                        {text("myskills.detail.description")}
                      </Text>
                      <Text class="text-xs text-foreground">{metadata().description}</Text>
                    </View>
                  )}
                </If>
              </View>

              <View class="gap-2">
                <If when={value().local.skill_md}>
                  {(markdown) => (
                    <TextView
                      markdown={skillBody(markdown())}
                      selectable
                      class="text-sm text-foreground"
                    />
                  )}
                </If>
                <If when={value().local.skill_md_error}>
                  {(error) => (
                    <Text class="text-xs text-destructive">
                      {`${text("myskills.detail.skillMdError")}: ${props.errorText(error())}`}
                    </Text>
                  )}
                </If>
                <If when={value().local.skill_md === null && value().local.skill_md_error === null}>
                  {() => (
                    <Text class="text-xs text-muted-foreground">
                      {text("myskills.detail.noSkillMd")}
                    </Text>
                  )}
                </If>
                <If when={value().local.state === "missing"}>
                  {() => (
                    <Text class="text-xs text-muted-foreground">
                      {text("myskills.detail.missingFilesHint")}
                    </Text>
                  )}
                </If>
              </View>
            </Scroll>
          )}
        </If>

        <View class="flex-row items-center justify-end gap-2">
          <If when={props.installation()}>
            {(item) => (
              <View class="flex-row items-center gap-2">
                <ActionButton
                  label={text("myskills.checkOne")}
                  variant="secondary"
                  disabled={props.busy()}
                  onPress={() => props.onCheck(item())}
                />
                <If when={isUpdateAvailable(item())}>
                  {() => (
                    <ActionButton
                      label={text("myskills.update")}
                      variant="primary"
                      disabled={props.busy()}
                      onPress={() => props.onUpdate(item())}
                    />
                  )}
                </If>
                <ActionButton
                  label={text("myskills.rename")}
                  variant="ghost"
                  disabled={props.busy()}
                  onPress={() => props.onRename(item())}
                />
                <IconButton
                  icon={trashIcon}
                  label={text("myskills.delete")}
                  tone="danger"
                  disabled={props.busy()}
                  onPress={() => props.onDelete(item())}
                />
              </View>
            )}
          </If>
        </View>
      </View>
    </Dialog>
  );
}
