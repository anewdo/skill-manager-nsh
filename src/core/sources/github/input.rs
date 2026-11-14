use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::SkillSource;
use crate::core::skill_path::safe_relative_path;
use crate::core::sources::skills_sh::marketplace_source;

#[derive(Debug)]
pub struct RepositoryInput {
    pub repository: String,
    pub location: Option<(String, String)>,
}

pub fn parse(input: &str) -> Result<RepositoryInput> {
    let input = input.trim();
    let path = if input.contains("://") {
        let url = reqwest::Url::parse(input)
            .map_err(|_| AppError::invalid_input("invalid GitHub URL"))?;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AppError::invalid_input(
                "enter a public HTTPS GitHub repository URL",
            ));
        }
        url.path().trim_matches('/').to_string()
    } else {
        input.trim_end_matches('/').to_string()
    };
    let mut parts = path.splitn(3, '/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    let base = marketplace_source(&format!("{owner}/{name}"), "skill")?;
    let location = match parts.next() {
        None => None,
        Some(rest) => {
            let (kind, tail) = rest.split_once('/').ok_or_else(|| {
                AppError::invalid_input("expected tree/<ref>/<path> or blob/<ref>/SKILL.md")
            })?;
            if !matches!(kind, "tree" | "blob") || tail.is_empty() {
                return Err(AppError::invalid_input("unsupported GitHub URL path"));
            }
            // 按 UTF-8 解码百分号，保留带斜杠引用；路径验证在引用拆分后进行。
            let mut bytes = Vec::new();
            let raw = tail.as_bytes();
            let mut index = 0;
            while index < raw.len() {
                if raw[index] == b'%' {
                    let hex = tail
                        .get(index + 1..index + 3)
                        .ok_or_else(|| AppError::invalid_input("invalid URL encoding"))?;
                    bytes.push(
                        u8::from_str_radix(hex, 16)
                            .map_err(|_| AppError::invalid_input("invalid URL encoding"))?,
                    );
                    index += 3;
                } else {
                    bytes.push(raw[index]);
                    index += 1;
                }
            }
            let tail = String::from_utf8(bytes)
                .map_err(|_| AppError::invalid_input("URL path must be UTF-8"))?;
            Some((kind.to_string(), tail))
        }
    };
    Ok(RepositoryInput {
        repository: base.repository.unwrap(),
        location,
    })
}

pub fn source(repository: &str, git_ref: Option<&str>, root: &str) -> Result<SkillSource> {
    let mut source = marketplace_source(repository, "skill")?;
    source.marketplace_skill_id = None;
    if !root.is_empty() {
        safe_relative_path(root)?;
    }
    if let Some(reference) = git_ref {
        let name = reference
            .strip_prefix("refs/heads/")
            .or_else(|| reference.strip_prefix("refs/tags/"))
            .ok_or_else(|| {
                AppError::invalid_input("expected a qualified branch or tag reference")
            })?;
        if name.is_empty()
            || reference.contains([' ', '\\', ':', '?', '*', '[', '~', '^'])
            || reference.contains("..")
            || reference.contains("@{")
            || reference.chars().any(char::is_control)
            || name.split('/').any(|part| {
                part.is_empty()
                    || part.starts_with('.')
                    || part.ends_with('.')
                    || part.ends_with(".lock")
            })
        {
            return Err(AppError::invalid_input("invalid Git reference"));
        }
    }
    let repository = source.repository.as_deref().unwrap();
    source.source_key = match git_ref {
        Some(reference) => format!(
            "github:{}",
            serde_json::to_string(&(repository, reference, root))?
        ),
        None => format!("github:{repository}/{root}"),
    };
    source.git_ref = git_ref.map(str::to_string);
    source.subpath = Some(root.to_string());
    Ok(source)
}

pub fn resolve(
    input: &RepositoryInput,
    refs: &[(String, String)],
    default: &str,
) -> Result<(String, String, bool)> {
    let Some((kind, tail)) = &input.location else {
        return Ok((default.to_string(), String::new(), false));
    };
    let matches: Vec<_> = refs
        .iter()
        .filter_map(|(reference, _)| {
            let short = reference
                .strip_prefix("refs/heads/")
                .or_else(|| reference.strip_prefix("refs/tags/"))?;
            let path = if tail == short {
                ""
            } else {
                tail.strip_prefix(short)?.strip_prefix('/')?
            };
            Some((reference.clone(), path.to_string()))
        })
        .collect();
    let [(reference, path)] = matches.as_slice() else {
        return Err(AppError::new(if matches.is_empty() { ErrorCode::NotFound } else { ErrorCode::AmbiguousSkill },
            if matches.is_empty() { "Git reference was not found" } else { "Git reference is ambiguous; use an unambiguous branch or tag URL" })
            .with_details(serde_json::json!({ "reason": if matches.is_empty() { "reference_not_found" } else { "ambiguous_reference" } })));
    };
    let single = kind == "blob";
    let root = if single {
        if path == "SKILL.md" {
            ""
        } else {
            path.strip_suffix("/SKILL.md")
                .ok_or_else(|| AppError::invalid_input("blob URL must point to SKILL.md"))?
        }
    } else {
        path
    };
    if !root.is_empty() {
        safe_relative_path(root)?;
    }
    Ok((reference.clone(), root.to_string(), single))
}
