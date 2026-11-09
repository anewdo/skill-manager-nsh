import { translate, type MessageKey, type MessageParams } from "../../i18n";
import type { AppStore } from "../../store";
import type { LocalState, UpdateState } from "../../types";
import type { BadgeTone } from "../../components/ui";
import type { StatusFilter, ViewMode } from "./model";

export type TextFn = (key: MessageKey, params?: MessageParams) => string;

export function createText(store: AppStore): TextFn {
  return (key, params) => translate(store.settings()?.language ?? "zh_cn")(key, params);
}

export const viewModeKeys: Record<ViewMode, MessageKey> = {
  list: "myskills.view.list",
  card: "myskills.view.card",
};

export const filterKeys: Record<StatusFilter, MessageKey> = {
  all: "myskills.filter.all",
  clean: "myskills.filter.clean",
  modified: "myskills.filter.modified",
  missing: "myskills.filter.missing",
  update_available: "myskills.filter.update_available",
};

export const localStateKeys: Record<LocalState, MessageKey> = {
  clean: "myskills.state.clean",
  modified: "myskills.state.modified",
  missing: "myskills.state.missing",
  unknown: "myskills.state.unknown",
};

export const updateStateKeys: Record<UpdateState, MessageKey> = {
  unknown: "myskills.updateState.unknown",
  up_to_date: "myskills.updateState.up_to_date",
  available: "myskills.updateState.available",
  check_failed: "myskills.updateState.check_failed",
};

export function localStateTone(state: LocalState): BadgeTone {
  switch (state) {
    case "modified":
      return "warning";
    case "missing":
      return "danger";
    case "clean":
      return "success";
    default:
      return "info";
  }
}

export function updateStateTone(state: UpdateState): BadgeTone {
  switch (state) {
    case "available":
      return "info";
    case "check_failed":
      return "warning";
    case "up_to_date":
      return "success";
    default:
      return "default";
  }
}

export const checkboxIndicatorClass =
  "h-4 w-4 items-center justify-center rounded border border-border bg-background checked:bg-primary indeterminate:bg-muted";

export const tabClass =
  "group rounded-(--radius) px-2.5 py-1 active:bg-accent selected:bg-secondary";
export const tabTextClass = "text-xs text-muted-foreground group-selected:text-foreground";
export const paginationItemClass =
  "group rounded-(--radius) px-2 py-1 text-xs text-muted-foreground active:bg-accent selected:bg-secondary selected:text-foreground disabled:opacity-40";
