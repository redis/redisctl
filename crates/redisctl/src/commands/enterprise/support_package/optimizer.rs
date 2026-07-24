//! Support package optimization module
//!
//! Reduces support package size by:
//! - Truncating log files to keep only recent entries
//! - Removing nested compressed files
//! - Filtering out redundant data

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use tar::{Archive, Builder, Header};

/// Options for optimizing support packages
#[derive(Debug, Clone)]
pub struct OptimizationOptions {
    /// Maximum number of lines to keep from the end of log files
    pub max_log_lines: usize,
    /// Whether to remove nested gzip files
    pub remove_nested_gz: bool,
    /// File patterns to exclude
    pub exclude_patterns: Vec<String>,
    /// Show verbose output during optimization
    pub verbose: bool,
}

impl Default for OptimizationOptions {
    fn default() -> Self {
        Self {
            max_log_lines: 1000,
            remove_nested_gz: true,
            exclude_patterns: vec![],
            verbose: false,
        }
    }
}

/// Optimize a support package (tar.gz format)
pub fn optimize_support_package(data: &[u8], options: &OptimizationOptions) -> Result<Vec<u8>> {
    if options.verbose {
        eprintln!("Starting optimization...");
        eprintln!("Original size: {} bytes", data.len());
    }

    // Decompress the tar.gz
    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);

    // Prepare output
    let output = Vec::new();
    let encoder = GzEncoder::new(output, Compression::default());
    let mut builder = Builder::new(encoder);

    let mut stats = OptimizationStats {
        files_processed: 0,
        files_truncated: 0,
        files_removed: 0,
    };

    // Patterns for nested gzip files
    let nested_gz_patterns = HashSet::from([".gz", ".tar.gz", ".tgz"]);

    // Process each entry in the tar
    for entry_result in archive.entries()? {
        let mut entry = entry_result.context("Failed to read tar entry")?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_string_lossy();

        stats.files_processed += 1;

        // Check if we should exclude this file
        if should_exclude(&path_str, options, &nested_gz_patterns) {
            if options.verbose {
                eprintln!("Removing: {}", path_str);
            }
            stats.files_removed += 1;
            continue;
        }

        // Check if this is a log file that should be truncated
        if is_log_file(&path_str) {
            if options.verbose {
                eprintln!("Truncating log: {}", path_str);
            }

            // Read the entire file
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;

            // Truncate to last N lines
            let truncated = truncate_log(&content, options.max_log_lines)?;

            if truncated.len() < content.len() {
                stats.files_truncated += 1;

                // Create new header with updated size
                let mut header = Header::new_gnu();
                header.set_path(&path)?;
                header.set_size(truncated.len() as u64);
                header.set_mode(entry.header().mode()?);
                header.set_cksum();

                builder.append(&header, truncated.as_slice())?;
            } else {
                // No truncation needed, copy as-is
                let header = entry.header().clone();
                builder.append(&header, &content[..])?;
            }
        } else {
            // Copy file as-is
            let header = entry.header().clone();
            builder.append(&header, &mut entry)?;
        }
    }

    // Finalize the tar archive
    builder.finish()?;
    let encoder = builder.into_inner()?;
    let optimized_data = encoder.finish()?;

    if options.verbose {
        eprintln!("Optimized size: {} bytes", optimized_data.len());
        eprintln!("Files processed: {}", stats.files_processed);
        eprintln!("Files truncated: {}", stats.files_truncated);
        eprintln!("Files removed: {}", stats.files_removed);

        let reduction = ((data.len() - optimized_data.len()) as f64 / data.len() as f64) * 100.0;
        eprintln!("Size reduction: {:.1}%", reduction);
    }

    Ok(optimized_data)
}

/// Check if a file should be excluded based on optimization options
fn should_exclude(
    path: &str,
    options: &OptimizationOptions,
    nested_gz_patterns: &HashSet<&str>,
) -> bool {
    // Check custom exclude patterns
    for pattern in &options.exclude_patterns {
        if path.contains(pattern) {
            return true;
        }
    }

    // Check if it's a nested gzip file
    if options.remove_nested_gz {
        // Don't remove if it's the root archive itself
        if path.contains('/') {
            for pattern in nested_gz_patterns {
                if path.ends_with(pattern) {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if a file is a log file based on extension or path
fn is_log_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    path_lower.ends_with(".log")
        || path_lower.contains("/logs/")
        || path_lower.contains("/log/")
        || path_lower.ends_with(".log.txt")
}

/// Truncate log content to keep only the last N lines
fn truncate_log(content: &[u8], max_lines: usize) -> Result<Vec<u8>> {
    let reader = BufReader::new(content);
    let mut lines: Vec<String> = reader
        .lines()
        .collect::<std::io::Result<Vec<_>>>()
        .context("Failed to read log lines")?;

    // Keep only the last N lines
    if lines.len() > max_lines {
        let skip = lines.len() - max_lines;
        lines.drain(0..skip);

        // Add a header indicating truncation
        let header = format!(
            "=== LOG TRUNCATED: Showing last {} of {} lines ===",
            max_lines,
            lines.len() + skip
        );
        lines.insert(0, header);
    }

    // Convert back to bytes
    let mut result = Vec::new();
    for line in lines {
        result.extend_from_slice(line.as_bytes());
        result.push(b'\n');
    }

    Ok(result)
}

/// Internal statistics tracking
struct OptimizationStats {
    files_processed: usize,
    files_truncated: usize,
    files_removed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_log_file() {
        assert!(is_log_file("redis.log"));
        assert!(is_log_file("/var/log/redis/redis.log"));
        assert!(is_log_file("logs/application.log"));
        assert!(is_log_file("error.log.txt"));
        assert!(!is_log_file("config.conf"));
        assert!(!is_log_file("data.json"));
    }

    #[test]
    fn test_truncate_log() {
        let content = b"line1\nline2\nline3\nline4\nline5\n";
        let truncated = truncate_log(content, 3).unwrap();
        let truncated_str = String::from_utf8(truncated).unwrap();

        // Should have header + 3 lines + trailing newline
        let lines: Vec<&str> = truncated_str.lines().collect();
        assert_eq!(lines.len(), 4); // header + 3 lines
        assert!(lines[0].contains("TRUNCATED"));
        assert_eq!(lines[1], "line3");
        assert_eq!(lines[2], "line4");
        assert_eq!(lines[3], "line5");
    }

    #[test]
    fn test_truncate_log_no_truncation_needed() {
        let content = b"line1\nline2\n";
        let truncated = truncate_log(content, 10).unwrap();
        let truncated_str = String::from_utf8(truncated).unwrap();

        // Should not add truncation header if no truncation needed
        let lines: Vec<&str> = truncated_str.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
    }

    #[test]
    fn test_should_exclude() {
        let options = OptimizationOptions {
            remove_nested_gz: true,
            exclude_patterns: vec!["backup".to_string()],
            ..Default::default()
        };
        let nested_patterns = HashSet::from([".gz", ".tar.gz"]);

        assert!(should_exclude(
            "data/archive.tar.gz",
            &options,
            &nested_patterns
        ));
        assert!(should_exclude(
            "logs/backup/file.log",
            &options,
            &nested_patterns
        ));
        assert!(!should_exclude("redis.log", &options, &nested_patterns));
    }
}
