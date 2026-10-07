//! Provenance-aware configuration editing, separate from the public serializable `Config`.

use super::config::write_owner_only;
use super::{Config, ConfigError};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// A literal TOML key or an array index. Dotted profile names are a single key, not split paths.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ConfigPathSegment {
    Key(String),
    Index(usize),
}

type ValuePath = Vec<ConfigPathSegment>;

/// Errors from provenance-aware configuration loading and saving.
/// Field paths and retained values are deliberately absent from conflict diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum ConfigDocumentError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("Configuration source changed since loading; reload before saving")]
    SourceChanged,
    #[error("Save destination already exists; load that document before editing it")]
    DestinationExists,
    #[error(
        "An environment-backed value was edited ambiguously; declare its literal replacement explicitly before saving"
    )]
    AmbiguousReference,
}

#[derive(Clone)]
struct Reference {
    original: String,
    resolved: String,
}

#[derive(Clone, PartialEq)]
struct SourceSnapshot {
    bytes: Option<Vec<u8>>,
    #[cfg(unix)]
    identity: Option<(u64, u64)>,
}

/// A loaded configuration together with its load-time reference and source snapshots.
///
/// Use this type for load/edit/save operations. `Config` remains an untracked serializable
/// value for embedding compatibility. A document never guesses provenance from the current
/// environment. Reference edits must be explicit; unrelated edits preserve original references.
///
/// Source checks detect changes observed before the write; they are not a filesystem transaction
/// or an atomic compare-and-swap against another concurrent writer.
///
/// ```no_run
/// use redisctl_core::{ConfigDocument, ConfigDocumentError, ConfigPathSegment};
/// # fn example() -> Result<(), ConfigDocumentError> {
/// let mut document = ConfigDocument::load()?;
/// // This caller intentionally changes this field, but not other fields' references.
/// document.allow_literal_replacement(&[ConfigPathSegment::Key("default_cloud".into())]);
/// document.config_mut().default_cloud = Some("production".into());
/// document.save()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct ConfigDocument {
    config: Config,
    source_path: PathBuf,
    source: SourceSnapshot,
    references: HashMap<ValuePath, Reference>,
    arrays: HashMap<ValuePath, toml::Value>,
    typed_snapshot: toml::Value,
    literal_replacements: HashSet<ValuePath>,
    // Values may survive in a caller's retained Profile after its original path is deleted or
    // explicitly replaced. Keep load origins for the lifetime of this document, independently
    // from the restoration paths that are pruned after a successful save.
    origin_values: HashSet<String>,
}

impl std::fmt::Debug for ConfigDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigDocument")
            .field("source_exists", &self.source.bytes.is_some())
            .finish_non_exhaustive()
    }
}

impl ConfigDocument {
    /// Load from the standard redisctl configuration location.
    pub fn load() -> Result<Self, ConfigDocumentError> {
        Self::load_from_path(&Config::config_path()?)
    }

    /// Load a configuration and capture its source and environment-reference provenance.
    /// Only a genuinely absent source is treated as an empty configuration.
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigDocumentError> {
        Self::load_with_context(path, &mut |var| std::env::var(var).ok())
    }

    fn load_with_context(
        path: &Path,
        context: &mut impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, ConfigDocumentError> {
        let source_path = absolute_path(path)?;
        let source = read_snapshot(&source_path)?;
        let Some(bytes) = &source.bytes else {
            return Ok(Self {
                config: Config::default(),
                source_path,
                source,
                references: HashMap::new(),
                arrays: HashMap::new(),
                typed_snapshot: toml::Value::try_from(Config::default())
                    .map_err(ConfigError::from)?,
                literal_replacements: HashSet::new(),
                origin_values: HashSet::new(),
            });
        };
        let content = std::str::from_utf8(bytes).map_err(|_| ConfigError::LoadError {
            path: source_path.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "Config must be UTF-8"),
        })?;
        let original: toml::Value = toml::from_str(content).map_err(ConfigError::from)?;
        let mut resolved = original.clone();
        Config::expand_string_values_with_context(&mut resolved, context);
        let config: Config = resolved.clone().try_into().map_err(ConfigError::from)?;
        // Match the serialized typed shape, so unknown fields remain outside this contract.
        let typed = toml::Value::try_from(&config).map_err(ConfigError::from)?;
        let mut references = HashMap::new();
        collect_references(
            &original,
            &resolved,
            &typed,
            &mut Vec::new(),
            &mut references,
        );
        let arrays = referenced_arrays(&typed, &references);
        let origin_values = references
            .values()
            .map(|reference| reference.resolved.clone())
            .collect();
        Ok(Self {
            config,
            source_path,
            source,
            references,
            arrays,
            typed_snapshot: typed,
            literal_replacements: HashSet::new(),
            origin_values,
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Edit normal fields. Changing an environment-backed field requires
    /// `allow_literal_replacement` before saving; removing a field/profile is allowed.
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Explicitly allow this exact string field to be written literally instead of restoring
    /// its original reference. Does not authorize replacements of other fields or array entries.
    /// A new destination for a moved/copied resolved reference also needs an explicit declaration.
    pub fn allow_literal_replacement(&mut self, path: &[ConfigPathSegment]) {
        self.literal_replacements.insert(path.to_vec());
    }

    /// Save back to the loaded path, preserving unchanged references.
    pub fn save(&mut self) -> Result<(), ConfigDocumentError> {
        self.save_to_path(&self.source_path.clone())
    }

    /// Save to the loaded path, or a new destination. Save-as still verifies the original source
    /// and refuses an existing destination; successful save-as tracks the new source afterward.
    pub fn save_to_path(&mut self, path: &Path) -> Result<(), ConfigDocumentError> {
        self.write(path, false)
    }

    /// As `save_to_path`, using the existing owner-only writer on Unix.
    pub fn save_to_path_owner_only(&mut self, path: &Path) -> Result<(), ConfigDocumentError> {
        self.write(path, true)
    }

    fn write(&mut self, path: &Path, owner_only: bool) -> Result<(), ConfigDocumentError> {
        self.write_with(path, |destination, content| {
            if owner_only {
                write_owner_only(destination, content)
            } else {
                fs::write(destination, content)
            }
        })
    }

    fn write_with(
        &mut self,
        path: &Path,
        writer: impl FnOnce(&Path, &[u8]) -> std::io::Result<()>,
    ) -> Result<(), ConfigDocumentError> {
        let destination = absolute_path(path)?;
        if read_snapshot(&self.source_path)? != self.source {
            return Err(ConfigDocumentError::SourceChanged);
        }
        if destination != self.source_path && read_snapshot(&destination)?.bytes.is_some() {
            return Err(ConfigDocumentError::DestinationExists);
        }
        let mut serialized = toml::Value::try_from(&self.config).map_err(ConfigError::from)?;
        // A mutable Config permits structural edits and copies. Do not guess a new origin by
        // value: fail closed if a new/changed untracked slot contains a loaded reference value.
        // Unchanged literal slots with coincidentally equal values keep their original meaning.
        for path in string_paths(&[], &serialized) {
            if self.literal_replacements.contains(&path) || self.references.contains_key(&path) {
                continue;
            }
            let current = value_at(&serialized, &path);
            if current != value_at(&self.typed_snapshot, &path)
                && current
                    .and_then(toml::Value::as_str)
                    .is_some_and(|value| self.origin_values.contains(value))
            {
                return Err(ConfigDocumentError::AmbiguousReference);
            }
        }
        // Array shape is ambiguous without explicit replacements. Referenced positions are
        // fixed; untracked string entries may change normally. Dropping every reference in an
        // array explicitly also removes its guard, permitting intentional array replacement.
        for (path, loaded_array) in &self.arrays {
            if !self.references.keys().any(|reference_path| {
                reference_path.starts_with(path)
                    && !self.literal_replacements.contains(reference_path)
            }) {
                continue;
            }
            let Some(current_array) = value_at(&serialized, path) else {
                continue;
            };
            let mut expected = loaded_array.clone();
            for reference_path in string_paths(path, loaded_array) {
                if (!self.references.contains_key(&reference_path)
                    || self.literal_replacements.contains(&reference_path))
                    && let Some(current) = value_at(&serialized, &reference_path)
                    && let Some(slot) = value_at_mut(&mut expected, &reference_path[path.len()..])
                {
                    *slot = current.clone();
                }
            }
            if current_array != &expected {
                return Err(ConfigDocumentError::AmbiguousReference);
            }
        }
        for (path, reference) in &self.references {
            if self.literal_replacements.contains(path) {
                continue;
            }
            if let Some(slot) = value_at_mut(&mut serialized, path) {
                if slot.as_str() != Some(reference.resolved.as_str()) {
                    return Err(ConfigDocumentError::AmbiguousReference);
                }
                *slot = toml::Value::String(reference.original.clone());
            }
        }
        let content = toml::to_string_pretty(&serialized).map_err(ConfigError::from)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::SaveError {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let result = writer(&destination, content.as_bytes());
        result.map_err(|source| ConfigError::SaveError {
            path: destination.display().to_string(),
            source,
        })?;
        let new_source = read_snapshot(&destination)?;
        if new_source.bytes.as_deref() != Some(content.as_bytes()) {
            return Err(ConfigDocumentError::SourceChanged);
        }
        // Deleted references must not spring back if a later edit reuses the same path.
        let typed = toml::Value::try_from(&self.config).map_err(ConfigError::from)?;
        self.references.retain(|path, _| {
            !self.literal_replacements.contains(path) && value_at(&typed, path).is_some()
        });
        self.arrays = referenced_arrays(&typed, &self.references);
        self.typed_snapshot = typed;
        self.literal_replacements.clear();
        self.source_path = destination;
        self.source = new_source;
        Ok(())
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, ConfigError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn read_snapshot(path: &Path) -> Result<SourceSnapshot, ConfigError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !path.is_symlink() => None,
        Err(source) => {
            return Err(ConfigError::LoadError {
                path: path.display().to_string(),
                source,
            });
        }
    };
    #[cfg(unix)]
    let identity = if bytes.is_some() {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path).map_err(|source| ConfigError::LoadError {
            path: path.display().to_string(),
            source,
        })?;
        Some((metadata.dev(), metadata.ino()))
    } else {
        None
    };
    Ok(SourceSnapshot {
        bytes,
        #[cfg(unix)]
        identity,
    })
}

fn collect_references(
    original: &toml::Value,
    resolved: &toml::Value,
    typed: &toml::Value,
    path: &mut ValuePath,
    references: &mut HashMap<ValuePath, Reference>,
) {
    match original {
        toml::Value::String(text) if text.contains('$') => {
            if let Some(value) = value_at(typed, path).and_then(toml::Value::as_str)
                && resolved.as_str() == Some(value)
            {
                references.insert(
                    path.clone(),
                    Reference {
                        original: text.clone(),
                        resolved: value.to_string(),
                    },
                );
            }
        }
        toml::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if let Some(resolved) = resolved.get(index) {
                    path.push(ConfigPathSegment::Index(index));
                    collect_references(value, resolved, typed, path, references);
                    path.pop();
                }
            }
        }
        toml::Value::Table(table) => {
            for (key, value) in table {
                if let Some(resolved) = resolved.get(key) {
                    path.push(ConfigPathSegment::Key(key.clone()));
                    collect_references(value, resolved, typed, path, references);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

fn value_at<'a>(mut value: &'a toml::Value, path: &[ConfigPathSegment]) -> Option<&'a toml::Value> {
    for segment in path {
        value = match segment {
            ConfigPathSegment::Key(key) => value.get(key)?,
            ConfigPathSegment::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

fn value_at_mut<'a>(
    mut value: &'a mut toml::Value,
    path: &[ConfigPathSegment],
) -> Option<&'a mut toml::Value> {
    for segment in path {
        value = match segment {
            ConfigPathSegment::Key(key) => value.get_mut(key)?,
            ConfigPathSegment::Index(index) => value.get_mut(*index)?,
        };
    }
    Some(value)
}

fn referenced_arrays(
    typed: &toml::Value,
    references: &HashMap<ValuePath, Reference>,
) -> HashMap<ValuePath, toml::Value> {
    let mut arrays = HashMap::new();
    for path in references.keys() {
        for (index, segment) in path.iter().enumerate() {
            if matches!(segment, ConfigPathSegment::Index(_))
                && let Some(array) = value_at(typed, &path[..index])
            {
                arrays.insert(path[..index].to_vec(), array.clone());
            }
        }
    }
    arrays
}

// Enumerate string slots rather than searching by value: equal values have independent origins.
fn string_paths(path: &[ConfigPathSegment], value: &toml::Value) -> Vec<ValuePath> {
    let mut paths = Vec::new();
    fn visit(value: &toml::Value, path: &mut ValuePath, paths: &mut Vec<ValuePath>) {
        match value {
            toml::Value::String(_) => paths.push(path.clone()),
            toml::Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    path.push(ConfigPathSegment::Index(index));
                    visit(value, path, paths);
                    path.pop();
                }
            }
            toml::Value::Table(table) => {
                for (key, value) in table {
                    path.push(ConfigPathSegment::Key(key.clone()));
                    visit(value, path, paths);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    visit(value, &mut path.to_vec(), &mut paths);
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use tempfile::TempDir;

    const FIXTURE: &str = r#"
files_api_key = "${DOC_GLOBAL:-fallback-global}"
[profiles."prod.eu"]
deployment_type = "cloud"
api_key = "${DOC_KEY}"
api_secret = "${DOC_SECRET}"
tags = ["$DOC_TAG", "literal", "${DOC_UNSET}"]
[profiles.stage]
deployment_type = "cloud"
api_key = "${DOC_OTHER_KEY}"
api_secret = "literal-secret"
[cloud_auth."prod.eu"]
okta_issuer = "${DOC_ISSUER:-https://issuer.example.invalid}"
okta_client_id = "literal-client"
sm_api_url = "https://sm.example.invalid"
"#;

    fn fixture() -> (TempDir, PathBuf, ConfigDocument) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, FIXTURE).unwrap();
        // Inject synthetic load-time values. The writer sees only normal process environment,
        // not this lookup: preservation cannot depend on expanding references again at save.
        let document = ConfigDocument::load_with_context(&path, &mut |name| match name {
            "DOC_KEY" | "DOC_OTHER_KEY" => Some("same-at-load".to_string()),
            "DOC_SECRET" => Some("synthetic\"secret\\nwith\nlines 🔑".to_string()),
            "DOC_TAG" => Some("synthetic-tag".to_string()),
            _ => None,
        })
        .unwrap();
        (directory, path, document)
    }

    fn field(profile: &str, field: &str) -> ValuePath {
        ["profiles", profile, field]
            .map(|key| ConfigPathSegment::Key(key.to_string()))
            .to_vec()
    }

    fn written(path: &Path) -> toml::Value {
        toml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn save(
        document: &mut ConfigDocument,
        path: &Path,
        owner_only: bool,
    ) -> Result<(), ConfigDocumentError> {
        if owner_only {
            document.save_to_path_owner_only(path)
        } else {
            document.save_to_path(path)
        }
    }

    fn assert_references(path: &Path) {
        let document = written(path);
        assert_eq!(
            document["profiles"]["prod.eu"]["api_key"].as_str(),
            Some("${DOC_KEY}")
        );
        assert_eq!(
            document["profiles"]["stage"]["api_key"].as_str(),
            Some("${DOC_OTHER_KEY}")
        );
        assert_eq!(
            document["profiles"]["prod.eu"]["api_secret"].as_str(),
            Some("${DOC_SECRET}")
        );
        assert_eq!(
            document["profiles"]["prod.eu"]["tags"][0].as_str(),
            Some("$DOC_TAG")
        );
        assert_eq!(
            document["profiles"]["prod.eu"]["tags"][2].as_str(),
            Some("${DOC_UNSET}")
        );
        assert_eq!(
            document["files_api_key"].as_str(),
            Some("${DOC_GLOBAL:-fallback-global}")
        );
        assert_eq!(
            document["cloud_auth"]["prod.eu"]["okta_issuer"].as_str(),
            Some("${DOC_ISSUER:-https://issuer.example.invalid}")
        );
        assert!(!fs::read_to_string(path).unwrap().contains("synthetic"));
    }

    #[test]
    fn both_writers_preserve_load_time_references_not_save_time_environment() {
        for owner_only in [false, true] {
            let (_directory, path, mut document) = fixture();
            assert_eq!(
                document.config().profiles["prod.eu"]
                    .cloud_credentials()
                    .unwrap()
                    .0,
                "same-at-load"
            );
            document.config_mut().default_cloud = Some("stage".to_string());
            save(&mut document, &path, owner_only).unwrap();
            assert_references(&path);
            // The post-save source snapshot works for a second edit, including owner-only rename.
            document.config_mut().default_cloud = Some("prod.eu".to_string());
            save(&mut document, &path, !owner_only).unwrap();
            assert_references(&path);
        }
    }

    #[test]
    fn save_as_preserves_references_and_tracks_the_new_source() {
        for owner_only in [false, true] {
            let (directory, path, mut document) = fixture();
            let destination = directory.path().join("export/config.toml");
            save(&mut document, &destination, owner_only).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
            assert_eq!(document.source_path(), destination);
            assert_references(&destination);
            document.config_mut().default_cloud = Some("stage".to_string());
            document.save().unwrap();
            assert_references(&destination);
        }
    }

    #[test]
    fn changed_deleted_or_invalid_source_is_not_overwritten_by_either_writer() {
        for owner_only in [false, true] {
            for replacement in [Some("default_cloud = 'external'"), Some("[[[invalid"), None] {
                let (directory, path, mut document) = fixture();
                match replacement {
                    Some(content) => fs::write(&path, content).unwrap(),
                    None => fs::remove_file(&path).unwrap(),
                }
                assert!(matches!(
                    save(&mut document, &path, owner_only),
                    Err(ConfigDocumentError::SourceChanged)
                ));
                assert_eq!(fs::read_to_string(&path).ok().as_deref(), replacement);
                let destination = directory.path().join("new-parent/config.toml");
                assert!(matches!(
                    save(&mut document, &destination, owner_only),
                    Err(ConfigDocumentError::SourceChanged)
                ));
                assert!(!destination.parent().unwrap().exists());
            }
        }
    }

    #[test]
    fn save_as_rejects_existing_destinations_without_touching_either_file() {
        let (directory, path, mut document) = fixture();
        let destination = directory.path().join("other.toml");
        fs::write(&destination, "keep this existing content").unwrap();
        for owner_only in [false, true] {
            assert!(matches!(
                save(&mut document, &destination, owner_only),
                Err(ConfigDocumentError::DestinationExists)
            ));
            assert_eq!(
                fs::read_to_string(&destination).unwrap(),
                "keep this existing content"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
        }
    }

    #[test]
    fn missing_source_can_be_created_but_an_external_creation_is_a_conflict() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new/config.toml");
        let mut document = ConfigDocument::load_from_path(&path).unwrap();
        assert!(document.config().profiles.is_empty());
        document.config_mut().files_api_key = Some("intentional-literal".to_string());
        document.save().unwrap();
        assert_eq!(
            written(&path)["files_api_key"].as_str(),
            Some("intentional-literal")
        );

        let path = directory.path().join("raced.toml");
        let mut document = ConfigDocument::load_from_path(&path).unwrap();
        fs::write(&path, "default_cloud = 'external'").unwrap();
        assert!(matches!(
            document.save(),
            Err(ConfigDocumentError::SourceChanged)
        ));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "default_cloud = 'external'"
        );
    }

    #[test]
    fn reference_replacement_is_explicit_and_field_specific() {
        for owner_only in [false, true] {
            let (_directory, path, mut document) = fixture();
            document
                .config_mut()
                .profiles
                .get_mut("prod.eu")
                .unwrap()
                .credentials = super::super::ProfileCredentials::Cloud {
                api_key: "intentional-literal-key".to_string(),
                api_secret: "synthetic\"secret\\nwith\nlines 🔑".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            };
            let error = save(&mut document, &path, owner_only).unwrap_err();
            assert!(matches!(error, ConfigDocumentError::AmbiguousReference));
            assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
            assert!(!error.to_string().contains("intentional-literal-key"));
            assert!(!format!("{error:?}").contains("DOC_KEY"));
            assert!(error.source().is_none());
            document.allow_literal_replacement(&field("prod.eu", "api_key"));
            save(&mut document, &path, owner_only).unwrap();
            let saved = written(&path);
            assert_eq!(
                saved["profiles"]["prod.eu"]["api_key"].as_str(),
                Some("intentional-literal-key")
            );
            assert_eq!(
                saved["profiles"]["prod.eu"]["api_secret"].as_str(),
                Some("${DOC_SECRET}")
            );
            assert_eq!(
                saved["profiles"]["stage"]["api_key"].as_str(),
                Some("${DOC_OTHER_KEY}")
            );
        }
    }

    #[test]
    fn array_shifts_and_ambiguous_edits_are_rejected_but_exact_replacements_work() {
        let (_directory, path, mut document) = fixture();
        document
            .config_mut()
            .profiles
            .get_mut("prod.eu")
            .unwrap()
            .tags
            .remove(1);
        assert!(matches!(
            document.save(),
            Err(ConfigDocumentError::AmbiguousReference)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);

        let (_directory, path, mut document) = fixture();
        document
            .config_mut()
            .profiles
            .get_mut("prod.eu")
            .unwrap()
            .tags
            .swap(0, 1);
        assert!(matches!(
            document.save(),
            Err(ConfigDocumentError::AmbiguousReference)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);

        let (_directory, path, mut document) = fixture();
        document
            .config_mut()
            .profiles
            .get_mut("prod.eu")
            .unwrap()
            .tags[0] = "intentional-tag".to_string();
        let mut tag = field("prod.eu", "tags");
        tag.push(ConfigPathSegment::Index(0));
        assert!(matches!(
            document.save(),
            Err(ConfigDocumentError::AmbiguousReference)
        ));
        document.allow_literal_replacement(&tag);
        document.save().unwrap();
        assert_eq!(
            written(&path)["profiles"]["prod.eu"]["tags"][0].as_str(),
            Some("intentional-tag")
        );
        assert_eq!(
            written(&path)["profiles"]["prod.eu"]["tags"][2].as_str(),
            Some("${DOC_UNSET}")
        );
    }

    #[test]
    fn removed_profile_references_do_not_reappear_on_later_reuse() {
        let (_directory, path, mut document) = fixture();
        let mut profile = document.config_mut().remove_profile("prod.eu").unwrap();
        document.save().unwrap();
        profile.credentials = super::super::ProfileCredentials::Cloud {
            api_key: "intentional-new-key".to_string(),
            api_secret: "intentional-new-secret".to_string(),
            api_url: "https://example.invalid".to_string(),
        };
        profile.tags.clear();
        document
            .config_mut()
            .set_profile("prod.eu".to_string(), profile);
        document.save().unwrap();
        assert_eq!(
            written(&path)["profiles"]["prod.eu"]["api_key"].as_str(),
            Some("intentional-new-key")
        );
        assert_eq!(
            written(&path)["profiles"]["stage"]["api_key"].as_str(),
            Some("${DOC_OTHER_KEY}")
        );
    }

    #[test]
    fn document_debug_does_not_reveal_config_source_keys_or_resolved_values() {
        let (_directory, _path, document) = fixture();
        for debug in [format!("{document:?}"), format!("{document:#?}")] {
            for marker in [
                "DOC_KEY",
                "prod.eu",
                "same-at-load",
                "synthetic",
                "literal-secret",
            ] {
                assert!(!debug.contains(marker));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn same_content_replaced_source_has_a_different_identity() {
        let (directory, path, mut document) = fixture();
        let replacement = directory.path().join("replacement");
        fs::write(&replacement, FIXTURE).unwrap();
        fs::rename(replacement, &path).unwrap();
        assert!(matches!(
            document.save(),
            Err(ConfigDocumentError::SourceChanged)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_save_as_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (directory, _path, mut document) = fixture();
        let destination = directory.path().join("owner-only.toml");
        document.save_to_path_owner_only(&destination).unwrap();
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_references(&destination);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_source_symlink_is_not_treated_as_an_empty_config() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::os::unix::fs::symlink(directory.path().join("missing-target"), &path).unwrap();
        assert!(matches!(
            ConfigDocument::load_from_path(&path),
            Err(ConfigDocumentError::Config(ConfigError::LoadError { .. }))
        ));
    }

    #[test]
    fn unreadable_source_type_does_not_overwrite_or_create_a_destination() {
        for owner_only in [false, true] {
            let (directory, path, mut document) = fixture();
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            let destination = directory.path().join("new/config.toml");
            assert!(matches!(
                save(&mut document, &destination, owner_only),
                Err(ConfigDocumentError::Config(ConfigError::LoadError { .. }))
            ));
            assert!(path.is_dir());
            assert!(!destination.parent().unwrap().exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_permission_failure_is_not_silently_ignored() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::process::Command::new("id").arg("-u").output().unwrap();
        if String::from_utf8_lossy(&root.stdout).trim() == "0" {
            eprintln!("permission-denial fixture unavailable when running as root");
            return;
        }
        for owner_only in [false, true] {
            let (_directory, path, mut document) = fixture();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            let result = save(&mut document, &path, owner_only);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(matches!(
                result,
                Err(ConfigDocumentError::Config(ConfigError::LoadError { .. }))
            ));
            assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
        }
    }

    #[test]
    fn explicit_replacement_of_all_array_references_allows_new_array_shape() {
        let (_directory, path, mut document) = fixture();
        for index in [0, 2] {
            let mut tag = field("prod.eu", "tags");
            tag.push(ConfigPathSegment::Index(index));
            document.allow_literal_replacement(&tag);
        }
        document
            .config_mut()
            .profiles
            .get_mut("prod.eu")
            .unwrap()
            .tags = vec!["intentional-new-tag".to_string()];
        document.save().unwrap();
        assert_eq!(
            written(&path)["profiles"]["prod.eu"]["tags"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            written(&path)["profiles"]["prod.eu"]["api_key"].as_str(),
            Some("${DOC_KEY}")
        );
    }

    #[test]
    fn moving_or_copying_loaded_profiles_requires_explicit_new_destinations() {
        for owner_only in [false, true] {
            for remove_original in [false, true] {
                let (_directory, path, mut document) = fixture();
                let profile = if remove_original {
                    document.config_mut().remove_profile("prod.eu").unwrap()
                } else {
                    document.config().profiles["prod.eu"].clone()
                };
                document
                    .config_mut()
                    .set_profile("renamed".to_string(), profile);
                assert!(matches!(
                    save(&mut document, &path, owner_only),
                    Err(ConfigDocumentError::AmbiguousReference)
                ));
                assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
                // A declaration at the old path does not authorize the new profile.
                document.allow_literal_replacement(&field("prod.eu", "api_key"));
                assert!(matches!(
                    save(&mut document, &path, owner_only),
                    Err(ConfigDocumentError::AmbiguousReference)
                ));
                assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
            }
        }
    }

    #[test]
    fn unchanged_literal_equal_to_a_reference_stays_literal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "files_api_key = '${DOC_KEY}'\ndefault_cloud = 'same-at-load'",
        )
        .unwrap();
        let mut document =
            ConfigDocument::load_with_context(&path, &mut |_| Some("same-at-load".to_string()))
                .unwrap();
        document.save().unwrap();
        assert_eq!(
            written(&path)["default_cloud"].as_str(),
            Some("same-at-load")
        );
        assert_eq!(written(&path)["files_api_key"].as_str(), Some("${DOC_KEY}"));
    }

    #[test]
    fn explicit_new_literal_destination_does_not_change_other_reference_origins() {
        let (_directory, path, mut document) = fixture();
        document.config_mut().default_cloud = Some("same-at-load".to_string());
        assert!(matches!(
            document.save(),
            Err(ConfigDocumentError::AmbiguousReference)
        ));
        document.allow_literal_replacement(&[ConfigPathSegment::Key("default_cloud".to_string())]);
        document.save().unwrap();
        assert_eq!(
            written(&path)["default_cloud"].as_str(),
            Some("same-at-load")
        );
        assert_references(&path);
        document.save().unwrap();
        assert_references(&path);
    }

    #[test]
    fn post_write_conflict_does_not_adopt_foreign_content_or_destination() {
        for owner_only in [false, true] {
            for save_as in [false, true] {
                let (directory, path, mut document) = fixture();
                let destination = if save_as {
                    directory.path().join("save-as.toml")
                } else {
                    path.clone()
                };
                let original_source = document.source.clone();
                // Deterministic local interleaving, not a timing-dependent race: the test writer
                // replaces the content before post-write verification gets to observe it.
                let result = document.write_with(&destination, |path, bytes| {
                    if owner_only {
                        write_owner_only(path, bytes)?;
                    } else {
                        fs::write(path, bytes)?;
                    }
                    fs::write(path, b"default_cloud = 'external-writer'")
                });
                assert!(matches!(result, Err(ConfigDocumentError::SourceChanged)));
                assert_eq!(document.source_path(), path);
                assert!(document.source == original_source);
                for _ in 0..2 {
                    let retry = save(&mut document, &destination, owner_only);
                    if save_as {
                        assert!(matches!(retry, Err(ConfigDocumentError::DestinationExists)));
                    } else {
                        assert!(matches!(retry, Err(ConfigDocumentError::SourceChanged)));
                    }
                    assert_eq!(
                        fs::read_to_string(&destination).unwrap(),
                        "default_cloud = 'external-writer'"
                    );
                }
            }
        }
    }

    #[test]
    fn load_origins_survive_deleted_or_explicitly_replaced_restoration_paths() {
        for owner_only in [false, true] {
            for delete_original in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("config.toml");
                fs::write(&path, "[profiles.original]\ndeployment_type = 'cloud'\napi_key = '${DOC_KEY}'\napi_secret = 'literal-secret'").unwrap();
                let mut document = ConfigDocument::load_with_context(&path, &mut |_| {
                    Some("synthetic-retained-key".to_string())
                })
                .unwrap();
                let retained = document.config().profiles["original"].clone();
                if delete_original {
                    document.config_mut().remove_profile("original");
                } else {
                    document.allow_literal_replacement(&field("original", "api_key"));
                }
                save(&mut document, &path, owner_only).unwrap();
                let first_save = fs::read_to_string(&path).unwrap();
                assert!(document.references.is_empty());
                document
                    .config_mut()
                    .set_profile("new-destination".to_string(), retained);
                assert!(matches!(
                    save(&mut document, &path, owner_only),
                    Err(ConfigDocumentError::AmbiguousReference)
                ));
                assert_eq!(fs::read_to_string(&path).unwrap(), first_save);
                // Intent at the destination is still needed after a previous successful save.
                document.allow_literal_replacement(&field("new-destination", "api_key"));
                save(&mut document, &path, owner_only).unwrap();
                assert_eq!(
                    written(&path)["profiles"]["new-destination"]["api_key"].as_str(),
                    Some("synthetic-retained-key")
                );
            }
        }
    }
}
