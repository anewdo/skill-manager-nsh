import type { MessageKey, Translate } from "./i18n";
import type { AppError, ErrorCode } from "./types";

const errorCodes: ErrorCode[] = [
  "INVALID_INPUT",
  "UNSAFE_PATH",
  "DATABASE",
  "IO",
  "NOT_FOUND",
  "CONFLICT",
  "NETWORK",
  "GIT",
  "INVALID_SKILL",
  "AMBIGUOUS_SKILL",
  "LOCAL_MODIFIED",
  "CANCELLED",
  "UNSUPPORTED",
  "INTERNAL",
];

function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === "string" && (errorCodes as string[]).includes(value);
}

/** The structured error the native module rejected with, kept as it is. */
export function toAppError(error: unknown): AppError {
  if (typeof error === "object" && error !== null) {
    const fields = error as Record<string, unknown>;
    const message = typeof fields.message === "string" ? fields.message : String(error);
    return {
      code: isErrorCode(fields.code) ? fields.code : "INTERNAL",
      message,
      retryable: fields.retryable === true,
      details: fields.details,
    };
  }
  return {
    code: "INTERNAL",
    message: typeof error === "string" ? error : String(error),
    retryable: false,
  };
}

export function describeError(error: AppError, t: Translate): string {
  const key = `error.${error.code.toLowerCase()}` as MessageKey;
  return `${t(key)}: ${error.message}`;
}

/** The installation a lineage conflict points at, when the backend named one. */
export function conflictInstallationId(error: AppError): string | null {
  if (error.code !== "CONFLICT" || typeof error.details !== "object" || error.details === null) {
    return null;
  }
  const id = (error.details as Record<string, unknown>).installationId;
  return typeof id === "string" ? id : null;
}
