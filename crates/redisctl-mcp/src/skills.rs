//! Embedded workflow catalog, shared by MCP resources and prompt entry points.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use tower_mcp::resource::ResourceBuilder;
use tower_mcp::{DynamicPromptRegistry, McpRouter, PromptBuilder};

use crate::server::parse_skill;

const BUNDLED: &[(&str, &str)] = &[
    (
        "cloud-database-provisioning",
        include_str!("../skills/cloud-database-provisioning/SKILL.md"),
    ),
    (
        "compare-approaches",
        include_str!("../skills/compare-approaches/SKILL.md"),
    ),
    (
        "data-explorer",
        include_str!("../skills/data-explorer/SKILL.md"),
    ),
    (
        "data-modeling-advisor",
        include_str!("../skills/data-modeling-advisor/SKILL.md"),
    ),
    (
        "enterprise-health-check",
        include_str!("../skills/enterprise-health-check/SKILL.md"),
    ),
    (
        "index-ab-test",
        include_str!("../skills/index-ab-test/SKILL.md"),
    ),
    (
        "index-advisor",
        include_str!("../skills/index-advisor/SKILL.md"),
    ),
    (
        "index-audit",
        include_str!("../skills/index-audit/SKILL.md"),
    ),
    (
        "index-migration",
        include_str!("../skills/index-migration/SKILL.md"),
    ),
    (
        "query-tuning",
        include_str!("../skills/query-tuning/SKILL.md"),
    ),
    (
        "redisctl-setup",
        include_str!("../skills/redisctl-setup/SKILL.md"),
    ),
];

struct Entry {
    markdown: String,
    source: &'static str,
}

fn checked_name(markdown: &str) -> Result<String> {
    let skill =
        parse_skill(markdown).context("Skill requires name, description and Markdown body")?;
    let name = &skill.name;
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || skill.description.trim().is_empty()
        || skill.body.trim().is_empty()
    {
        bail!(
            "Invalid skill metadata; use a lowercase hyphenated name and nonempty description/body"
        );
    }
    Ok(skill.name)
}

fn catalog(directory: Option<&Path>) -> Result<BTreeMap<String, Entry>> {
    let mut entries = BTreeMap::new();
    for &(name, markdown) in BUNDLED {
        let parsed_name = checked_name(markdown).context("Invalid embedded skill")?;
        if name != parsed_name {
            bail!("Embedded skill name does not match its catalog entry");
        }
        entries.insert(
            name.to_string(),
            Entry {
                markdown: markdown.to_string(),
                source: "bundled",
            },
        );
    }
    if let Some(directory) = directory {
        let mut paths = std::fs::read_dir(directory)
            .context("Cannot read explicit skills directory")?
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort_by_key(std::fs::DirEntry::file_name);
        let mut custom_names = HashSet::new();
        for path in paths {
            if !path.file_type()?.is_dir() {
                continue;
            }
            let skill_path = path.path().join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }
            let markdown = std::fs::read_to_string(&skill_path)
                .with_context(|| format!("Cannot read skill {}", skill_path.display()))?;
            let name = checked_name(&markdown)
                .with_context(|| format!("Invalid skill {}", skill_path.display()))?;
            if !custom_names.insert(name.clone()) {
                bail!("Duplicate name in explicit skills directory: {name}");
            }
            entries.insert(
                name,
                Entry {
                    markdown,
                    source: "custom",
                },
            );
        }
    }
    Ok(entries)
}

pub(crate) fn register(
    mut router: McpRouter,
    prompts: &DynamicPromptRegistry,
    directory: Option<&Path>,
    enabled_toolsets: Vec<String>,
    available_tools: Vec<String>,
) -> Result<McpRouter> {
    let entries = catalog(directory)?;
    let mut index = Vec::new();
    for (name, entry) in entries {
        let skill = parse_skill(&entry.markdown).context("Invalid skill metadata")?;
        let uri = format!("redisctl://skills/{name}");
        index.push(serde_json::json!({
            "name": name, "description": skill.description, "uri": uri,
            "prompt": name, "source": entry.source
        }));
        router = router.resource(
            ResourceBuilder::new(&uri)
                .name(&name)
                .description(&skill.description)
                .mime_type("text/markdown")
                .text(entry.markdown),
        );
        let body = format!(
            "Before following this workflow, read redisctl://skills to check the current tool availability. \
             Call show_policy when available and respect policy denials; never invoke unavailable \
             tools or enable writes merely to follow a skill.\n\n{}",
            skill.body
        );
        let description = skill.description;
        prompts.register(
            PromptBuilder::new(&name)
                .description(&description)
                .handler(move |_arguments| {
                    let body = body.clone();
                    let description = description.clone();
                    async move {
                        Ok(tower_mcp::GetPromptResult::user_message_with_description(
                            body,
                            description,
                        ))
                    }
                })
                .build(),
        );
    }
    let index = serde_json::json!({
        "schema_version": 1,
        "instructions": "Choose a relevant skill and read its URI. Workflows are guidance, not permissions. Check available_tools and show_policy when available before invoking tools. Do not invoke unavailable tools. Credentials must be entered locally, never in chat. redisctl-setup guides first-run configuration.",
        "enabled_toolsets": enabled_toolsets,
        "available_tools": available_tools,
        "skills": index
    });
    Ok(router.resource(
        ResourceBuilder::new("redisctl://skills")
            .name("Redisctl skill library")
            .description("Workflow index and current tool availability; start with redisctl-setup for onboarding.")
            .mime_type("application/json")
            .text(index.to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_matches_skill_directories() {
        let catalog = catalog(None).unwrap();
        let mut disk = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("skills"))
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        disk.sort();
        assert_eq!(disk, catalog.keys().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn custom_skills_override_by_name_and_keep_bundled_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let skill = directory.path().join("override");
        std::fs::create_dir(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: data-explorer\ndescription: Custom explorer\n---\nCustom workflow.",
        )
        .unwrap();
        let entries = catalog(Some(directory.path())).unwrap();
        assert_eq!(entries["data-explorer"].source, "custom");
        assert!(
            entries["data-explorer"]
                .markdown
                .contains("Custom workflow")
        );
        assert_eq!(entries["redisctl-setup"].source, "bundled");
    }

    #[test]
    fn invalid_overrides_are_reported() {
        let directory = tempfile::tempdir().unwrap();
        let skill = directory.path().join("invalid");
        std::fs::create_dir(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: ../escape\ndescription: Invalid\n---\nBody",
        )
        .unwrap();
        assert!(catalog(Some(directory.path())).is_err());
        assert!(catalog(Some(&directory.path().join("missing"))).is_err());
    }

    #[test]
    fn new_custom_names_extend_catalog_and_duplicates_fail() {
        let directory = tempfile::tempdir().unwrap();
        for child in ["first", "second"] {
            std::fs::create_dir(directory.path().join(child)).unwrap();
        }
        let markdown = "---\nname: custom-workflow\ndescription: Custom workflow\n---\nBody";
        std::fs::write(directory.path().join("first/SKILL.md"), markdown).unwrap();
        let entries = catalog(Some(directory.path())).unwrap();
        assert_eq!(entries.len(), BUNDLED.len() + 1);
        assert_eq!(entries["custom-workflow"].source, "custom");
        std::fs::write(directory.path().join("second/SKILL.md"), markdown).unwrap();
        let error = catalog(Some(directory.path())).err().unwrap().to_string();
        assert!(error.contains("Duplicate name"));
    }

    #[test]
    fn metadata_requires_safe_names_description_and_body() {
        for name in [
            "../escape",
            "Uppercase",
            "-leading",
            "trailing-",
            "two--hyphens",
            "",
        ] {
            assert!(
                checked_name(&format!("---\nname: {name}\ndescription: Valid\n---\nBody")).is_err()
            );
        }
        assert!(checked_name("---\nname: valid\ndescription: \"\"\n---\nBody").is_err());
        assert!(checked_name("---\nname: valid\ndescription: Valid\n---\n").is_err());
        assert!(checked_name("name: valid\ndescription: Valid\nBody").is_err());
    }
}
