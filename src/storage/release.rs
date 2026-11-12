//! Release records stored under `.tasks/@releases/<id>.yml` (DEV-393).
//!
//! A release is built like a sprint: a numeric ID from its file name, the
//! plan (version, label, notes, planned date), when it was released, its
//! history and any keys this version does not know. Which tickets belong to
//! a release is kept on the tickets (`releases: [<id>]`), so a ticket keeps
//! its releases when it is moved, renamed or archived.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[cfg(feature = "schema")]
use schemars::JsonSchema;

use crate::types::TaskChangeLogEntry;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Release {
    /// The version this release ships, as written (`0.15.0`, `v0.15.0`).
    #[serde(default)]
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    /// The day the release is planned for (`YYYY-MM-DD`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_date: Option<String>,
    /// When the release went out (RFC 3339); absent while it is planned.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub released_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub history: Vec<TaskChangeLogEntry>,

    /// User-owned YAML extensions, preserved across typed edits.
    #[serde(flatten, default, serialize_with = "serialize_extra_fields")]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra_fields: BTreeMap<String, serde_yaml_ng::Value>,
}

fn is_builtin_release_key(key: &str) -> bool {
    matches!(key, "version" | "label" | "notes" | "planned_date" | "released_at" | "created" | "modified" | "history")
}

fn serialize_extra_fields<S: serde::Serializer>(fields: &BTreeMap<String, serde_yaml_ng::Value>, serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    for (key, value) in fields {
        if !is_builtin_release_key(key) {
            map.serialize_entry(key, value)?;
        }
    }
    map.end()
}

impl Release {
    /// Trim text fields and drop empty ones before writing.
    pub fn canonicalize(&mut self) {
        self.version = self.version.trim().to_string();
        for field in [&mut self.label, &mut self.notes, &mut self.planned_date, &mut self.released_at] {
            if let Some(value) = field.take() {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    *field = Some(trimmed.to_string());
                }
            }
        }
        self.history.retain(|entry| !entry.changes.is_empty());
    }

    pub fn is_released(&self) -> bool {
        self.released_at.is_some()
    }

    /// The parsed version, when the stored text is one.
    pub fn parsed_version(&self) -> Option<crate::utils::version::Version> {
        crate::utils::version::Version::parse(&self.version).ok()
    }

    /// `0.15.0` or `0.15.0 (Spring)`.
    pub fn display_name(&self) -> String {
        match self.label.as_deref() {
            Some(label) if label != self.version => format!("{} ({label})", self.version),
            _ => self.version.clone(),
        }
    }

    pub fn to_yaml(&self) -> Result<String, serde_yaml_ng::Error> {
        serde_yaml_ng::to_string(self)
    }

    /// The file of release `id` under a tasks root.
    pub fn path_for_id(tasks_root: &Path, id: u32) -> PathBuf {
        Self::dir(tasks_root).join(format!("{id}.yml"))
    }

    /// The release directory (`.tasks/@releases`).
    pub fn dir(tasks_root: &Path) -> PathBuf {
        tasks_root.join("@releases")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let yaml = "version: v0.15.0\nlabel: Spring\nplanned_date: '2026-12-01'\nowner_team: core\n";
        let mut release: Release = serde_yaml_ng::from_str(yaml).unwrap();
        release.canonicalize();
        assert_eq!(release.version, "v0.15.0");
        assert_eq!(release.display_name(), "v0.15.0 (Spring)");
        assert!(!release.is_released());
        let written = release.to_yaml().unwrap();
        assert!(written.contains("owner_team: core"), "{written}");
        let again: Release = serde_yaml_ng::from_str(&written).unwrap();
        assert_eq!(again, release);
    }

    #[test]
    fn canonicalize_drops_blank_text() {
        let mut release = Release { version: " 1.0 ".into(), label: Some("  ".into()), notes: Some(" n ".into()), ..Release::default() };
        release.canonicalize();
        assert_eq!(release.version, "1.0");
        assert_eq!(release.label, None);
        assert_eq!(release.notes.as_deref(), Some("n"));
    }
}
