//! `redisctl init` - onboard a project to Redis services and make its AI coding
//! agent Redis-fluent.
//!
//! This module is the step sequence; each concern lives in its own submodule.

mod output;
mod project;
mod util;

use std::sync::OnceLock;

use crate::cli::InitArgs;
use crate::error::RedisCtlError;
use output::{bold, dim, ok, yellow};

/// What counts as a connection string, wherever one arrives: the --url flag or a
/// bare positional (the Redis Cloud console hands out `redis-cli -u <url>`; accept
/// that paste whole and pull the URL out of it).
fn url_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"rediss?://[^\s"']+"#).expect("static regex"))
}

/// Pull a Redis URL out of pasted text. `Ok(None)` when the text is blank,
/// an error when there is text but no URL in it.
fn extract_url(pasted: &str) -> Result<Option<String>, RedisCtlError> {
    if let Some(m) = url_regex().find(pasted) {
        return Ok(Some(m.as_str().to_string()));
    }
    if pasted.trim().is_empty() {
        return Ok(None);
    }
    Err(RedisCtlError::InvalidInput {
        message: format!("no redis:// or rediss:// URL found in: {}", pasted.trim()),
    })
}

pub async fn run(args: &InitArgs) -> Result<(), RedisCtlError> {
    let pasted = [args.url.clone().unwrap_or_default()]
        .into_iter()
        .chain(args.pasted.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let given_url = extract_url(&pasted)?;

    output::banner();
    let dry = args.dry_run;
    println!(
        "{}",
        bold(&format!(
            "\nredisctl init{}\n",
            if dry {
                " (dry run - nothing will be written)"
            } else {
                ""
            }
        ))
    );

    let cwd = std::env::current_dir().map_err(|e| RedisCtlError::FileError {
        path: ".".into(),
        message: e.to_string(),
    })?;
    let proj = project::detect(&cwd);
    let mut descriptor = proj.runtime.as_str().to_string();
    if let Some(pm) = proj.pm {
        descriptor.push_str(&format!(", {pm}"));
    }
    if let Some(framework) = &proj.framework {
        descriptor.push_str(&format!(", {framework}"));
    }
    println!(
        "{}   {} {}",
        bold("Project"),
        proj.name,
        dim(&format!("({descriptor})"))
    );

    let agents = project::choose_agents(&args.agents, &project::detect_agents(&cwd));
    let agent_bits = proj
        .agent_markers
        .iter()
        .map(|(marker, found)| format!("{marker} {}", if *found { ok("✓") } else { dim("✗") }))
        .collect::<Vec<_>>()
        .join("  ");
    let names = agents
        .iter()
        .map(|a| a.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "{}    {}{}   {}\n",
        bold("Agents"),
        names,
        if args.agents.is_empty() {
            dim(" (detected)")
        } else {
            String::new()
        },
        dim(&format!("existing: {agent_bits}"))
    );
    if proj.runtime == project::Runtime::Unknown {
        println!(
            "{}",
            yellow(
                "  note: no package manifest detected - continuing; everything redisctl init writes is language-agnostic.\n"
            )
        );
    }

    // No step records changes yet, so the summary prints a subject with no lines.
    let subject = given_url.as_deref().map(|url| {
        format!(
            "database: {}{} via provided URL",
            util::mask_url(url),
            args.name
                .as_deref()
                .map(|n| format!(" [{n}]"))
                .unwrap_or_default()
        )
    });
    println!(
        "{}{}",
        bold(if dry { "Plan" } else { "Changes" }),
        subject
            .map(|s| format!("  {}", dim(&format!("({s})"))))
            .unwrap_or_default()
    );

    if dry {
        println!(
            "{}",
            dim("\nDry run complete. Run again without --dry-run to apply.\n")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_paste_means_no_url() {
        assert_eq!(extract_url("").unwrap(), None);
        assert_eq!(extract_url("   ").unwrap(), None);
    }

    #[test]
    fn raw_urls_pass_through() {
        assert_eq!(
            extract_url("redis://localhost:6379").unwrap().as_deref(),
            Some("redis://localhost:6379")
        );
        assert_eq!(
            extract_url("rediss://h:12000").unwrap().as_deref(),
            Some("rediss://h:12000")
        );
    }

    #[test]
    fn pasted_connect_command_yields_its_url() {
        assert_eq!(
            extract_url("redis-cli -u redis://default:pw@host:12000")
                .unwrap()
                .as_deref(),
            Some("redis://default:pw@host:12000")
        );
    }

    #[test]
    fn quotes_delimit_the_url() {
        assert_eq!(
            extract_url(r#"redis-cli -u "rediss://h:1""#)
                .unwrap()
                .as_deref(),
            Some("rediss://h:1")
        );
    }

    #[test]
    fn text_without_a_url_is_an_error_naming_the_text() {
        let err = extract_url("garbage in").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no redis:// or rediss:// URL found"), "{msg}");
        assert!(msg.contains("garbage in"), "{msg}");
    }
}
