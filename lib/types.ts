export type SourceType = "github" | "well_known";
export type LocalState = "unknown" | "clean" | "modified" | "missing";
export type UpdateState = "unknown" | "up_to_date" | "available" | "check_failed";
export type UpdateConflictPolicy = "reject" | "overwrite";
export type OperationState = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export type OperationKind =
  | "install"
  | "check_updates"
  | "apply_update"
  | "delete"
  | "rename";
export type Language = "zh_cn" | "en";
export type Theme = "light" | "dark" | "system";
export type ProxyMode = "system" | "none" | "custom";
export type LeaderboardKind = "all_time" | "trending" | "hot";

export type ErrorCode =
  | "INVALID_INPUT"
  | "UNSAFE_PATH"
  | "DATABASE"
  | "IO"
  | "NOT_FOUND"
  | "CONFLICT"
  | "NETWORK"
  | "GIT"
  | "INVALID_SKILL"
  | "AMBIGUOUS_SKILL"
  | "LOCAL_MODIFIED"
  | "CANCELLED"
  | "UNSUPPORTED"
  | "INTERNAL";

export interface AppError {
  code: ErrorCode;
  message: string;
  retryable: boolean;
  details?: unknown;
}

export interface OperationProgress {
  phase: string;
  current?: number;
  total?: number;
  bytes?: number;
  bytes_total?: number;
  detail?: string;
  cancellable: boolean;
}

export interface OperationLabel {
  name: string;
  source: string;
}

export interface OperationSnapshot {
  id: string;
  kind: OperationKind;
  state: OperationState;
  cancel_requested: boolean;
  label?: OperationLabel;
  progress: OperationProgress | null;
  result: unknown;
  error: AppError | null;
  created_at: number;
  updated_at: number;
}

export type OperationEventKind =
  | "operation-state"
  | "operation-progress"
  | "library-changed"
  | "operations-resync";

export interface OperationEvent {
  kind: OperationEventKind;
  operation: OperationSnapshot | null;
}

export interface SkillSource {
  source_type: SourceType;
  source_key: string;
  repository?: string;
  git_ref?: string;
  subpath?: string;
  url?: string;
  digest?: string;
}

export interface InstalledSkill {
  id: string;
  display_name: string;
  root_relpath: string;
  root_key: string;
  source: SkillSource | null;
  installed_revision: string | null;
  remote_revision: string | null;
  baseline_hash: string | null;
  current_hash: string | null;
  local_state: LocalState;
  update_state: UpdateState;
  update_checked_at: number | null;
  created_at: number;
  updated_at: number;
  last_error: string | null;
}

/** A finished marketplace install, plus a same-named local copy the app left untouched. */
export interface InstallResult extends InstalledSkill {
  unmanaged_copy?: string | null;
}

export interface SkillMetadata {
  name: string;
  description: string;
}

export interface LocalSkillInfo {
  state: LocalState;
  path: string;
  current_hash: string | null;
  file_count: number | null;
  total_bytes: number | null;
  metadata: SkillMetadata | null;
  skill_md: string | null;
  skill_md_error: AppError | null;
  inspected_at: number;
}

export interface SkillDetail {
  skill: InstalledSkill;
  local: LocalSkillInfo;
}

export interface DeletedItem {
  id: string;
  display_name: string;
}

export interface BatchItemFailure {
  id: string;
  error: AppError;
}

export interface DeleteReport {
  deleted: DeletedItem[];
  failed: BatchItemFailure[];
  cancelled: boolean;
}

export interface UpdateCheckItem {
  id: string;
  display_name: string;
  update_state: UpdateState;
  remote_revision: string | null;
}

export interface CheckUpdatesReport {
  checked: UpdateCheckItem[];
  failed: BatchItemFailure[];
  cancelled: boolean;
}

export interface ProxySettings {
  mode: ProxyMode;
  custom_url: string | null;
}

export interface ProxyPatch {
  mode?: ProxyMode;
  custom_url?: string;
}

export interface Settings {
  language: Language;
  theme: Theme;
  proxy: ProxySettings;
  show_market_avatars: boolean;
}

export interface SettingsPatch {
  language?: Language;
  theme?: Theme;
  proxy?: ProxyPatch;
  show_market_avatars?: boolean;
}

export interface CacheClearReport {
  cleared_entries: number;
}

/** The primary display's work area in points, `y` growing down. */
export interface DisplayWorkArea {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type ConnectivityTarget = "github" | "skills_sh";

export interface ConnectivityCheck {
  target: ConnectivityTarget;
  ok: boolean;
  latency_ms: number;
  error: string | null;
}

export interface MarketplaceSkill {
  id: string;
  skill_id: string;
  name: string;
  source: string;
  installs: number;
  lineage: SkillSource;
}

export interface SkillPreview {
  source: SkillSource;
  relative_root: string;
  name: string;
  description: string;
  skill_md: string;
  revision: string | null;
  digest: string | null;
}
