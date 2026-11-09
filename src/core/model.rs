use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::{AppError, Result};
use super::metadata::SkillMetadata;

macro_rules! string_enum {
    ($type:ty, {$($variant:ident => $value:literal),+ $(,)?}) => {
        impl $type {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }

            pub fn parse(value: &str) -> Result<Self> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(AppError::database(format!(
                        "invalid {} value: {value}",
                        stringify!($type)
                    ))),
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Marketplace,
    Local,
}

string_enum!(Category, {
    Marketplace => "marketplace",
    Local => "local"
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "well_known")]
    WellKnown,
}

string_enum!(SourceType, {
    GitHub => "github",
    WellKnown => "well_known"
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalState {
    Unknown,
    Clean,
    Modified,
    Missing,
}

string_enum!(LocalState, {
    Unknown => "unknown",
    Clean => "clean",
    Modified => "modified",
    Missing => "missing"
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    Unknown,
    UpToDate,
    Available,
    CheckFailed,
}

string_enum!(UpdateState, {
    Unknown => "unknown",
    UpToDate => "up_to_date",
    Available => "available",
    CheckFailed => "check_failed"
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Install,
    CheckUpdates,
    ApplyUpdate,
    Delete,
    Rename,
    Restore,
    PurgeTrash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationProgress {
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub cancellable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationSnapshot {
    pub id: String,
    pub kind: OperationKind,
    pub state: OperationState,
    pub cancel_requested: bool,
    pub progress: Option<OperationProgress>,
    pub result: Option<Value>,
    pub error: Option<AppError>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationEventKind {
    OperationState,
    OperationProgress,
    InstallationsChanged,
    OperationsResync,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub kind: OperationEventKind,
    pub operation: Option<OperationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillSource {
    pub source_type: SourceType,
    pub source_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installation {
    pub id: String,
    pub category: Category,
    pub display_name: String,
    pub root_relpath: String,
    pub root_key: String,
    pub source: Option<SkillSource>,
    pub installed_revision: Option<String>,
    pub remote_revision: Option<String>,
    pub baseline_hash: Option<String>,
    pub current_hash: Option<String>,
    pub local_state: LocalState,
    pub update_state: UpdateState,
    pub update_checked_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSkillInfo {
    pub state: LocalState,
    pub path: String,
    pub current_hash: Option<String>,
    pub file_count: Option<u64>,
    pub total_bytes: Option<u64>,
    pub metadata: Option<SkillMetadata>,
    pub skill_md: Option<String>,
    pub skill_md_error: Option<AppError>,
    pub inspected_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallationDetail {
    pub installation: Installation,
    pub local: LocalSkillInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrashEntry {
    pub id: String,
    pub installation_id: String,
    pub category: Category,
    pub display_name: String,
    pub original_relpath: String,
    pub original_root_key: String,
    #[serde(skip_serializing)]
    pub trash_relpath: String,
    #[serde(skip_serializing)]
    pub installation: Installation,
    pub deleted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletedItem {
    pub id: String,
    pub display_name: String,
    pub trashed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoredItem {
    pub trash_id: String,
    pub installation_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchItemFailure {
    pub id: String,
    pub error: AppError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteReport {
    pub deleted: Vec<DeletedItem>,
    pub failed: Vec<BatchItemFailure>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoreReport {
    pub restored: Vec<RestoredItem>,
    pub failed: Vec<BatchItemFailure>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurgeReport {
    pub purged: Vec<String>,
    pub failed: Vec<BatchItemFailure>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    ZhCn,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    #[default]
    System,
    None,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    pub custom_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub theme: Theme,
    pub proxy: ProxySettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketCacheEntry {
    pub cache_key: String,
    pub payload: Value,
    pub fetched_at: i64,
    pub last_accessed_at: i64,
}
