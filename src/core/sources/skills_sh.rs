use std::collections::HashSet;
use std::io::Read;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::model::{ProxySettings, SkillSource, SourceType};

const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const NEXT_DATA_MARKER: &str = r#"<script id="__NEXT_DATA__" type="application/json">"#;

static EMBEDDED_SKILL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:\\)?\"source(?:\\)?\":(?:\\)?\"(?P<source>[^"\\]+)(?:\\)?\",(?:[^{}]|\\.)*?(?:(?:\\)?\"skillId(?:\\)?\"|(?:\\)?\"skill_id(?:\\)?\"):(?:\\)?\"(?P<skill_id>[^"\\]+)(?:\\)?\",(?:[^{}]|\\.)*?(?:\\)?\"name(?:\\)?\":(?:\\)?\"(?P<name>[^"\\]*)(?:\\)?\",(?:[^{}]|\\.)*?(?:\\)?\"installs(?:\\)?\":(?P<installs>\d+)"#,
    )
    .expect("embedded skills.sh pattern is valid")
});

static LEGACY_SKILL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"\{"source":"(?P<source>[^"]+)","skill_id":"(?P<skill_id>[^"]+)"(?:,"name":"(?P<name>[^"]*)")?(?:.*?"installs":(?P<installs>\d+))?\}"#,
    )
    .expect("legacy skills.sh pattern is valid")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaderboardKind {
    AllTime,
    Trending,
    Hot,
}

impl LeaderboardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllTime => "all_time",
            Self::Trending => "trending",
            Self::Hot => "hot",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "all_time" => Ok(Self::AllTime),
            "trending" => Ok(Self::Trending),
            "hot" => Ok(Self::Hot),
            _ => Err(AppError::invalid_input("unknown leaderboard kind")),
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::AllTime => "https://skills.sh/",
            Self::Trending => "https://skills.sh/trending",
            Self::Hot => "https://skills.sh/hot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceSkill {
    pub id: String,
    pub skill_id: String,
    pub name: String,
    pub source: String,
    pub installs: u64,
}

impl MarketplaceSkill {
    pub fn lineage(&self) -> Result<SkillSource> {
        marketplace_source(&self.source, &self.skill_id)
    }
}

pub fn marketplace_source(source: &str, skill_id: &str) -> Result<SkillSource> {
    let source = source.trim();
    let skill_id = normalize_skill_id(skill_id)?;
    if source.contains('/') {
        github_source(source, skill_id)
    } else {
        well_known_source(source, skill_id)
    }
}

fn github_source(source: &str, skill_id: &str) -> Result<SkillSource> {
    let mut segments = source.split('/');
    let (Some(owner), Some(repository), None) = (segments.next(), segments.next(), segments.next())
    else {
        return Err(AppError::invalid_input(
            "GitHub source must use the owner/repository form",
        ));
    };
    if !valid_github_owner(owner) {
        return Err(AppError::invalid_input("invalid GitHub repository owner"));
    }
    let repository = repository
        .strip_suffix(".git")
        .unwrap_or(repository)
        .to_ascii_lowercase();
    if !valid_github_repository(&repository) {
        return Err(AppError::invalid_input("invalid GitHub repository name"));
    }

    let owner = owner.to_ascii_lowercase();
    let repository = format!("{owner}/{repository}");
    Ok(SkillSource {
        source_type: SourceType::GitHub,
        source_key: format!("github:{repository}/{skill_id}"),
        repository: Some(repository.clone()),
        git_ref: None,
        subpath: Some(skill_id.to_string()),
        url: Some(format!("https://github.com/{repository}.git")),
        digest: None,
    })
}

fn well_known_source(domain: &str, skill_id: &str) -> Result<SkillSource> {
    let domain = domain.to_ascii_lowercase();
    if !valid_domain(&domain) {
        return Err(AppError::invalid_input("invalid skills.sh website domain"));
    }
    Ok(SkillSource {
        source_type: SourceType::WellKnown,
        source_key: format!("well_known:{domain}/{skill_id}"),
        repository: None,
        git_ref: None,
        subpath: None,
        url: Some(format!("https://skills.sh/site/{domain}/{skill_id}")),
        digest: None,
    })
}

fn normalize_skill_id(skill_id: &str) -> Result<&str> {
    let skill_id = skill_id.trim();
    if skill_id.is_empty()
        || skill_id.contains(['\\', ':', '?', '#'])
        || skill_id.chars().any(char::is_control)
        || skill_id
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(AppError::invalid_input("invalid skills.sh skill ID"));
    }
    Ok(skill_id)
}

fn valid_github_owner(owner: &str) -> bool {
    !owner.is_empty()
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn valid_github_repository(repository: &str) -> bool {
    !repository.is_empty()
        && !matches!(repository, "." | "..")
        && repository.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
}

#[derive(Clone)]
pub struct SkillsShClient {
    client: Client,
}

impl SkillsShClient {
    pub fn new(proxy: &ProxySettings) -> Result<Self> {
        Ok(Self {
            client: http::build_client(proxy, REQUEST_TIMEOUT)?,
        })
    }

    pub fn fetch_leaderboard(&self, kind: LeaderboardKind) -> Result<Vec<MarketplaceSkill>> {
        let body = self.get_text(kind.url())?;
        parse_leaderboard_html(&body)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MarketplaceSkill>> {
        let query = query.trim();
        if query.is_empty() {
            return Err(AppError::invalid_input("search query is required"));
        }
        if !(1..=300).contains(&limit) {
            return Err(AppError::invalid_input(
                "search limit must be between 1 and 300",
            ));
        }
        let mut url = reqwest::Url::parse("https://skills.sh/api/search")
            .expect("the skills.sh search URL is valid");
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &limit.to_string());
        let body = self.get_text(url.as_str())?;
        parse_search_response(&body)
    }

    fn get_text(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|error| network_error(url, error))?
            .error_for_status()
            .map_err(|error| network_error(url, error))?;
        read_response(response, url)
    }
}

fn network_error(url: &str, error: reqwest::Error) -> AppError {
    let retryable = error.is_timeout()
        || error.is_connect()
        || error.status().is_some_and(|status| {
            status.as_u16() == 408 || status.as_u16() == 429 || status.is_server_error()
        });
    AppError::new(
        ErrorCode::Network,
        format!("skills.sh request failed for {url}: {error}"),
    )
    .retryable(retryable)
}

fn read_response(response: Response, url: &str) -> Result<String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(invalid_response(
            "skills.sh response exceeds the size limit",
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Network,
                format!("failed to read skills.sh response from {url}: {error}"),
            )
            .retryable(true)
        })?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(invalid_response(
            "skills.sh response exceeds the size limit",
        ));
    }
    String::from_utf8(bytes).map_err(|_| invalid_response("skills.sh response is not UTF-8"))
}

pub fn parse_leaderboard_html(html: &str) -> Result<Vec<MarketplaceSkill>> {
    if let Some(skills) = parse_next_data(html)? {
        return Ok(skills);
    }

    let skills = parse_embedded_with_regex(html, &EMBEDDED_SKILL_PATTERN)?;
    if !skills.is_empty() {
        return Ok(skills);
    }
    let skills = parse_embedded_with_regex(html, &LEGACY_SKILL_PATTERN)?;
    if !skills.is_empty() {
        return Ok(skills);
    }
    Err(invalid_response(
        "skills.sh leaderboard does not contain a recognized skill list",
    ))
}

fn parse_next_data(html: &str) -> Result<Option<Vec<MarketplaceSkill>>> {
    let Some(marker) = html.find(NEXT_DATA_MARKER) else {
        return Ok(None);
    };
    let start = marker + NEXT_DATA_MARKER.len();
    let end = html[start..]
        .find("</script>")
        .map(|offset| start + offset)
        .ok_or_else(|| invalid_response("skills.sh __NEXT_DATA__ script is not closed"))?;
    let data: Value = serde_json::from_str(&html[start..end])
        .map_err(|error| invalid_response(format!("invalid skills.sh __NEXT_DATA__: {error}")))?;
    let skills = data
        .pointer("/props/pageProps/initialSkills")
        .or_else(|| data.pointer("/props/pageProps/skills"))
        .or_else(|| data.pointer("/props/pageProps/items"))
        .and_then(Value::as_array);
    skills.map(|skills| parse_skills_array(skills)).transpose()
}

pub fn parse_search_response(body: &str) -> Result<Vec<MarketplaceSkill>> {
    let payload: Value = serde_json::from_str(body)
        .map_err(|error| invalid_response(format!("invalid skills.sh search response: {error}")))?;
    let skills = payload
        .as_array()
        .or_else(|| payload.get("skills").and_then(Value::as_array))
        .ok_or_else(|| invalid_response("skills.sh search response is missing skills"))?;
    parse_skills_array(skills)
}

fn parse_skills_array(items: &[Value]) -> Result<Vec<MarketplaceSkill>> {
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for item in items {
        let source = required_string(item, "source")?;
        let skill_id = ["skillId", "skill_id", "id"]
            .into_iter()
            .find_map(|key| item.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| invalid_response("skills.sh entry is missing skillId"))?;
        let name = match item.get("name") {
            None | Some(Value::Null) => skill_id,
            Some(Value::String(name)) if name.trim().is_empty() => skill_id,
            Some(Value::String(name)) => name.trim(),
            Some(_) => return Err(invalid_response("skills.sh entry has an invalid name")),
        };
        let installs = match item.get("installs") {
            None | Some(Value::Null) => 0,
            Some(value) => value
                .as_u64()
                .ok_or_else(|| invalid_response("skills.sh entry has invalid installs"))?,
        };
        let skill = marketplace_skill(source, skill_id, name, installs)?;
        if seen.insert(skill.id.clone()) {
            skills.push(skill);
        }
    }
    Ok(skills)
}

fn parse_embedded_with_regex(html: &str, pattern: &Regex) -> Result<Vec<MarketplaceSkill>> {
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for captures in pattern.captures_iter(html) {
        let source = captures["source"].replace(r#"\""#, "\"");
        let skill_id = captures["skill_id"].replace(r#"\""#, "\"");
        let name = captures
            .name("name")
            .map(|value| value.as_str().replace(r#"\""#, "\""))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| skill_id.clone());
        let installs = captures
            .name("installs")
            .and_then(|value| value.as_str().parse::<u64>().ok())
            .unwrap_or(0);
        let skill = marketplace_skill(&source, &skill_id, name.trim(), installs)?;
        if seen.insert(skill.id.clone()) {
            skills.push(skill);
        }
    }
    Ok(skills)
}

fn marketplace_skill(
    source: &str,
    skill_id: &str,
    name: &str,
    installs: u64,
) -> Result<MarketplaceSkill> {
    let lineage = marketplace_source(source, skill_id).map_err(|error| {
        invalid_response(format!(
            "skills.sh entry has an invalid source: {}",
            error.message
        ))
    })?;
    let source = match lineage.source_type {
        SourceType::GitHub => lineage
            .repository
            .clone()
            .expect("GitHub lineage contains a repository"),
        SourceType::WellKnown => source.trim().to_ascii_lowercase(),
    };
    Ok(MarketplaceSkill {
        id: lineage.source_key,
        skill_id: skill_id.trim().to_string(),
        name: name.to_string(),
        source,
        installs,
    })
}

fn required_string<'a>(item: &'a Value, key: &str) -> Result<&'a str> {
    item.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_response(format!("skills.sh entry is missing {key}")))
}

fn invalid_response(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::Network, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_next_data_payload() {
        let html = r#"
        <html>
          <script id="__NEXT_DATA__" type="application/json">
            {"props":{"pageProps":{"initialSkills":[{"source":"Antfu/Skills.git","skillId":"vite","name":"","installs":152}]}}}
          </script>
        </html>
        "#;

        let skills = parse_leaderboard_html(html).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "github:antfu/skills/vite");
        assert_eq!(skills[0].source, "antfu/skills");
        assert_eq!(skills[0].name, "vite");
        assert_eq!(skills[0].installs, 152);
    }

    #[test]
    fn parses_current_rsc_payload_and_deduplicates_lineage() {
        let html = r#"
        <script>self.__next_f.push([1,"[{\"source\":\"anthropics/skills\",\"skillId\":\"template-skill\",\"name\":\"template-skill\",\"installs\":238},{\"source\":\"Anthropics/Skills.git\",\"skillId\":\"template-skill\",\"name\":\"duplicate\",\"installs\":1},{\"source\":\"vercel/ai\",\"skillId\":\"ai-sdk\",\"name\":\"ai-sdk\",\"installs\":265}]"])</script>
        "#;

        let skills = parse_leaderboard_html(html).unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id, "github:anthropics/skills/template-skill");
        assert_eq!(skills[1].id, "github:vercel/ai/ai-sdk");
    }

    #[test]
    fn parses_legacy_embedded_payload() {
        let html = r#"
        {"source":"openai/skills","skill_id":"playwright","name":"playwright","installs":2}
        "#;

        let skills = parse_leaderboard_html(html).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "github:openai/skills/playwright");
    }

    #[test]
    fn parses_search_array_and_object_payloads() {
        let entry = r#"{"source":"uizze.com","skillId":"ui-radar","name":"UI Radar","installs":8}"#;
        for body in [format!("[{entry}]"), format!(r#"{{"skills":[{entry}]}}"#)] {
            let skills = parse_search_response(&body).unwrap();
            assert_eq!(skills[0].id, "well_known:uizze.com/ui-radar");
            assert_eq!(
                skills[0].lineage().unwrap().url.unwrap(),
                "https://skills.sh/site/uizze.com/ui-radar"
            );
        }
        assert!(parse_search_response("[]").unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_responses_instead_of_returning_an_empty_list() {
        assert!(parse_leaderboard_html("<html>app shell</html>").is_err());
        assert!(
            parse_leaderboard_html(
                r#"<script id="__NEXT_DATA__" type="application/json">not JSON</script>"#
            )
            .is_err()
        );
        assert!(parse_search_response(r#"{"items":[]}"#).is_err());
        assert!(parse_search_response(r#"[{"source":"owner/repo"}]"#).is_err());
        assert!(
            parse_search_response(r#"[{"source":"owner/repo","skillId":"skill","installs":-1}]"#)
                .is_err()
        );
    }

    #[test]
    fn normalizes_github_and_website_sources() {
        let github = marketplace_source("Owner/Repo.git", "nested/skill").unwrap();
        assert_eq!(github.source_key, "github:owner/repo/nested/skill");
        assert_eq!(github.repository.as_deref(), Some("owner/repo"));
        assert_eq!(github.subpath.as_deref(), Some("nested/skill"));
        assert_eq!(
            github.url.as_deref(),
            Some("https://github.com/owner/repo.git")
        );

        let website = marketplace_source("WWW.Example.COM", "ui").unwrap();
        assert_eq!(website.source_type, SourceType::WellKnown);
        assert_eq!(website.source_key, "well_known:www.example.com/ui");
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_sources() {
        for (source, skill_id) in [
            ("owner/repo/extra", "skill"),
            ("-owner/repo", "skill"),
            ("owner/repo", "../skill"),
            ("owner/repo", "nested//skill"),
            ("owner/repo", "C:skill"),
            ("bad_domain", "skill"),
            ("example.com.", "skill"),
            ("https://example.com", "skill"),
        ] {
            assert!(
                marketplace_source(source, skill_id).is_err(),
                "{source}/{skill_id}"
            );
        }
    }
}
