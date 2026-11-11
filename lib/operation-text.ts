import type { MessageKey, Translate } from "../i18n";
import { formatBytes, progressPercent } from "./format";
import type { OperationKind, OperationSnapshot } from "./types";

const kindKeys: Record<Exclude<OperationKind, "install">, MessageKey> = {
  check_updates: "operation.kind.check_updates",
  apply_update: "operation.kind.apply_update",
  delete: "operation.kind.delete",
  rename: "operation.kind.rename",
};

const phaseKeys: Record<string, MessageKey> = {
  downloading: "operation.phase.downloading",
  validating: "operation.phase.validating",
  installing: "operation.phase.installing",
  renaming: "operation.phase.renaming",
  deleting: "operation.phase.deleting",
  checking: "operation.phase.checking",
};

const gitStepKeys: Record<string, MessageKey> = {
  "starting git": "operation.git.starting",
  "repository metadata": "operation.git.metadata",
  "selecting skill": "operation.git.selecting",
  "preparing files": "operation.git.preparing",
  "reading skill metadata": "operation.git.reading",
};

/** What an operation is, in a few words: the Skill an install fetches, else what it does. */
export function operationTitle(t: Translate, snapshot: OperationSnapshot): string {
  if (snapshot.kind === "install") {
    return snapshot.label?.name ?? t("operation.unnamed");
  }
  return t(kindKeys[snapshot.kind]);
}

function phaseLabel(t: Translate, phase: string): string {
  const key = phaseKeys[phase];
  return key === undefined ? phase : t(key);
}

/** The step the backend reported in English, in the user's language when it is a known one. */
function stepLabel(t: Translate, detail: string): string {
  const key = gitStepKeys[detail];
  if (key !== undefined) {
    return t(key);
  }
  const transfer = /^(Receiving objects|Writing files) \((\d+)\/(\d+)\)$/.exec(detail);
  if (transfer === null) {
    return detail;
  }
  return t(
    transfer[1] === "Receiving objects" ? "operation.git.receiving" : "operation.git.writing",
    { current: transfer[2], total: transfer[3] },
  );
}

/**
 * Where an operation is, as one line: its state while it waits, is being
 * cancelled or failed; else its phase with the numbers that phase has.
 * `speed` is its transfer rate in bytes per second, when known.
 */
export function operationDetail(
  t: Translate,
  snapshot: OperationSnapshot,
  speed?: number,
): string {
  if (snapshot.state === "queued") {
    return t("operation.waiting");
  }
  if (snapshot.cancel_requested) {
    return t("operation.cancelling");
  }
  if (snapshot.state === "failed") {
    return snapshot.error === null ? t("operation.failed") : snapshot.error.message;
  }
  const progress = snapshot.progress;
  if (progress === null) {
    return t("operation.working");
  }
  const parts = [phaseLabel(t, progress.phase)];
  if (progress.phase === "downloading") {
    const percent = progressPercent(progress);
    if (percent !== undefined) {
      parts.push(`${Math.round(percent)}%`);
    }
    if (progress.detail !== undefined) {
      parts.push(stepLabel(t, progress.detail));
    }
    if (speed !== undefined && speed > 0) {
      parts.push(`${formatBytes(speed)}/s`);
    }
    if (progress.bytes !== undefined) {
      const total =
        progress.bytes_total === undefined
          ? t("operation.unknownTotal")
          : formatBytes(progress.bytes_total);
      parts.push(`${formatBytes(progress.bytes)} / ${total}`);
    }
    return parts.join(" · ");
  }
  if (progress.current !== undefined && progress.total !== undefined) {
    parts.push(`${progress.current}/${progress.total}`);
  }
  if (progress.detail !== undefined) {
    parts.push(stepLabel(t, progress.detail));
  }
  return parts.join(" · ");
}

/** Whether the user may still ask an operation to stop. */
export function isCancellable(snapshot: OperationSnapshot): boolean {
  if (snapshot.cancel_requested) {
    return false;
  }
  return (
    snapshot.state === "queued" ||
    (snapshot.state === "running" && snapshot.progress?.cancellable === true)
  );
}
