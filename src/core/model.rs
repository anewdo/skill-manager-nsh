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
    ImportLocal,
    ImportUrl,
    CheckUpdates,
    ApplyUpdate,
    Delete,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationProgress {
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Bytes received so far, when the transport reports them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Total bytes, when the transport knows them before the transfer ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub cancellable: bool,
}

/// What the UI calls an operation before its result names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationLabel {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationSnapshot {
    pub id: String,
    pub kind: OperationKind,
    pub state: OperationState,
    pub cancel_requested: bool,
    /// The marketplace entry the operation belongs to, when the caller named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<OperationLabel>,
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
    LibraryChanged,
    OperationsResync,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub kind: OperationEventKind,
    pub operation: Option<OperationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSource {
    pub source_type: SourceType,
    pub source_key: String,
    /// 市场条目标识；实际目录解析后仍保留，用于关联市场安装状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marketplace_skill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// URL 来源保存完整 refs/heads 或 refs/tags；市场来源为空，跟随当前默认分支。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// 已解析的仓库内目录；仓库根为 ""，尚未解析的市场条目为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub id: String,
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
pub struct SkillDetail {
    pub skill: InstalledSkill,
    pub local: LocalSkillInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletedItem {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateConflictPolicy {
    Reject,
    Overwrite,
}

impl UpdateConflictPolicy {
    pub fn from_request(value: &str) -> Result<Self> {
        match value {
            "reject" => Ok(Self::Reject),
            "overwrite" => Ok(Self::Overwrite),
            _ => Err(AppError::invalid_input(format!(
                "unsupported update conflict policy: {value}"
            ))),
        }
    }
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
pub struct UpdateCheckItem {
    pub id: String,
    pub display_name: String,
    pub update_state: UpdateState,
    pub remote_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckUpdatesReport {
    pub checked: Vec<UpdateCheckItem>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub theme: Theme,
    pub proxy: ProxySettings,
    #[serde(default = "show_market_avatars_default")]
    pub show_market_avatars: bool,
}

fn show_market_avatars_default() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: Theme::default(),
            proxy: ProxySettings::default(),
            show_market_avatars: show_market_avatars_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ProxyPatch {
    #[serde(default)]
    pub mode: Option<ProxyMode>,
    #[serde(default)]
    pub custom_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct SettingsPatch {
    #[serde(default)]
    pub language: Option<Language>,
    #[serde(default)]
    pub theme: Option<Theme>,
    #[serde(default)]
    pub proxy: Option<ProxyPatch>,
    #[serde(default)]
    pub show_market_avatars: Option<bool>,
}

impl SettingsPatch {
    pub fn apply_to(self, current: &Settings) -> Settings {
        let proxy = match self.proxy {
            Some(patch) => ProxySettings {
                mode: patch.mode.unwrap_or(current.proxy.mode),
                custom_url: patch
                    .custom_url
                    .or_else(|| current.proxy.custom_url.clone()),
            },
            None => current.proxy.clone(),
        };
        Settings {
            language: self.language.unwrap_or(current.language),
            theme: self.theme.unwrap_or(current.theme),
            proxy,
            show_market_avatars: self
                .show_market_avatars
                .unwrap_or(current.show_market_avatars),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheClearReport {
    pub cleared_entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectivityCheck {
    pub target: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(value: serde_json::Value) -> SettingsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn empty_patch_keeps_the_current_settings() {
        let current = Settings::default();
        assert_eq!(patch(serde_json::json!({})).apply_to(&current), current);
    }

    #[test]
    fn partial_patches_merge_fields() {
        let current = Settings {
            language: Language::En,
            theme: Theme::Dark,
            proxy: ProxySettings {
                mode: ProxyMode::Custom,
                custom_url: Some("http://127.0.0.1:7897".to_string()),
            },
            ..Settings::default()
        };

        let merged = patch(serde_json::json!({ "theme": "light" })).apply_to(&current);
        assert_eq!(merged.language, Language::En);
        assert_eq!(merged.theme, Theme::Light);
        assert_eq!(merged.proxy, current.proxy);
    }

    #[test]
    fn avatars_are_shown_unless_the_patch_turns_them_off() {
        let current = Settings::default();
        assert!(current.show_market_avatars);

        let merged = patch(serde_json::json!({ "show_market_avatars": false })).apply_to(&current);
        assert!(!merged.show_market_avatars);
        assert_eq!(
            patch(serde_json::json!({ "theme": "dark" })).apply_to(&merged),
            Settings {
                theme: Theme::Dark,
                show_market_avatars: false,
                ..current
            }
        );
    }

    #[test]
    fn settings_saved_before_avatars_existed_keep_them_on() {
        let saved = serde_json::from_value::<Settings>(serde_json::json!({
            "language": "en",
            "theme": "dark",
            "proxy": { "mode": "system", "custom_url": null }
        }))
        .unwrap();

        assert_eq!(saved.language, Language::En);
        assert!(saved.show_market_avatars);
    }

    #[test]
    fn proxy_patch_keeps_the_previous_url_when_it_is_omitted() {
        let current = Settings {
            proxy: ProxySettings {
                mode: ProxyMode::Custom,
                custom_url: Some("http://127.0.0.1:7897".to_string()),
            },
            ..Settings::default()
        };

        let merged = patch(serde_json::json!({ "proxy": { "mode": "system" } })).apply_to(&current);
        assert_eq!(merged.proxy.mode, ProxyMode::System);
        assert_eq!(
            merged.proxy.custom_url.as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }

    #[test]
    fn unknown_enum_values_do_not_deserialize() {
        for value in [
            serde_json::json!({ "language": "fr" }),
            serde_json::json!({ "theme": "blue" }),
            serde_json::json!({ "proxy": { "mode": "proxy" } }),
        ] {
            assert!(serde_json::from_value::<SettingsPatch>(value).is_err());
        }
    }
}
