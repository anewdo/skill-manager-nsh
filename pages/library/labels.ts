import type { Tone } from "../../components/ui";
import type { MessageKey } from "../../i18n";
import type { LocalState, UpdateState } from "../../lib/types";
import type { StatusFilter, ViewMode } from "./model";

export const viewModeKeys: Record<ViewMode, MessageKey> = {
  list: "library.view.list",
  card: "library.view.card",
};

export const filterKeys: Record<StatusFilter, MessageKey> = {
  all: "library.filter.all",
  clean: "library.filter.clean",
  modified: "library.filter.modified",
  missing: "library.filter.missing",
  update_available: "library.filter.update_available",
};

export const localStateKeys: Record<LocalState, MessageKey> = {
  clean: "library.state.clean",
  modified: "library.state.modified",
  missing: "library.state.missing",
  unknown: "library.state.unknown",
};

export const updateStateKeys: Record<UpdateState, MessageKey> = {
  unknown: "library.updateState.unknown",
  up_to_date: "library.updateState.up_to_date",
  available: "library.updateState.available",
  check_failed: "library.updateState.check_failed",
};

export const localStateTone: Record<LocalState, Tone> = {
  clean: "success",
  modified: "warning",
  missing: "danger",
  unknown: "neutral",
};

export const updateStateTone: Record<UpdateState, Tone> = {
  unknown: "neutral",
  up_to_date: "neutral",
  available: "info",
  check_failed: "warning",
};
