use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(feature = "schema")]
use schemars::JsonSchema;

use crate::types::{TaskChange, TaskChangeLogEntry};

/// Warning raised during sprint canonicalization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SprintCanonicalizationWarning {
    /// `plan.length` was provided together with `plan.ends_at`; `plan.length` was dropped.
    LengthDiscardedInFavorOfEndsAt,
}

impl SprintCanonicalizationWarning {
    /// Stable machine-readable identifier for the warning variant.
    pub fn code(&self) -> &'static str {
        match self {
            SprintCanonicalizationWarning::LengthDiscardedInFavorOfEndsAt => {
                "length_discarded_in_favor_of_ends_at"
            }
        }
    }

    /// Human-friendly explanation suitable for CLI output.
    pub fn message(&self) -> &'static str {
        match self {
            SprintCanonicalizationWarning::LengthDiscardedInFavorOfEndsAt => {
                "plan.length was ignored because plan.ends_at was provided."
            }
        }
    }
}

/// Canonical representation of a sprint file stored under `.tasks/@sprints/<id>.yml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Sprint {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan: Option<SprintPlan>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual: Option<SprintActual>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tasks: Vec<SprintTaskEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub history: Vec<SprintHistoryEntry>,
}

impl Sprint {
    /// Apply canonicalization rules before persisting to disk.
    pub fn canonicalize(&mut self) -> Vec<SprintCanonicalizationWarning> {
        let mut warnings = Vec::new();
        if let Some(plan) = &mut self.plan {
            warnings.extend(plan.canonicalize());
            if plan.is_empty() {
                self.plan = None;
            }
        }

        if let Some(actual) = &mut self.actual {
            actual.canonicalize();
            if actual.is_empty() {
                self.actual = None;
            }
        }

        self.canonicalize_tasks();

        // Drop empty history entries to keep diffs tidy.
        self.history.retain(|entry| {
            !entry.at.is_empty() || entry.actor.is_some() || !entry.changes.is_empty()
        });
        self.history.iter_mut().for_each(|entry| {
            entry.changes.retain(|change| {
                change.old.is_some() || change.new.is_some() || !change.field.is_empty()
            });
        });
        warnings
    }

    /// Serialize the sprint into canonical YAML.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Compute the file path for a sprint ID relative to a tasks root.
    pub fn path_for_id(tasks_root: &Path, sprint_id: u32) -> PathBuf {
        Self::dir(tasks_root).join(format!("{}.yml", sprint_id))
    }

    /// Return the sprint storage directory (`.tasks/@sprints`).
    pub fn dir(tasks_root: &Path) -> PathBuf {
        let target = tasks_root.join("@sprints");
        if target.exists() {
            return target;
        }

        let legacy = tasks_root.join("sprints");
        if legacy.exists() {
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match fs::rename(&legacy, &target) {
                Ok(_) => return target,
                Err(err) => {
                    eprintln!(
                        "[lotar][warn] failed to migrate legacy .tasks/sprints directory: {}",
                        err
                    );
                    return legacy;
                }
            }
        }

        target
    }

    fn canonicalize_tasks(&mut self) {
        if self.tasks.is_empty() {
            return;
        }

        let mut seen: HashSet<String> = HashSet::new();
        let mut deduped: Vec<SprintTaskEntry> = Vec::with_capacity(self.tasks.len());

        for mut entry in self.tasks.drain(..) {
            entry.id = entry.id.trim().to_string();
            if entry.id.is_empty() {
                continue;
            }
            if seen.insert(entry.id.clone()) {
                deduped.push(entry);
            }
        }

        for entry in &mut deduped {
            entry.order = None;
        }

        self.tasks = deduped;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintTaskEntry {
    pub id: String,
    pub order: Option<u32>,
}

impl Serialize for SprintTaskEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.order.is_none() {
            return serializer.serialize_str(self.id.as_str());
        }

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        if let Some(order) = &self.order {
            map.serialize_entry("order", order)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SprintTaskEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EntryVisitor;

        impl<'de> Visitor<'de> for EntryVisitor {
            type Value = SprintTaskEntry;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sprint task entry as a string or map")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SprintTaskEntry {
                    id: value.to_string(),
                    order: None,
                })
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SprintTaskEntry {
                    id: value,
                    order: None,
                })
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut id: Option<String> = None;
                let mut order: Option<u32> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        "order" => {
                            if order.is_some() {
                                return Err(de::Error::duplicate_field("order"));
                            }
                            order = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<IgnoredAny>()?;
                        }
                    }
                }

                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                Ok(SprintTaskEntry { id, order })
            }
        }

        deserializer.deserialize_any(EntryVisitor)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintPlan {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity: Option<SprintCapacity>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
}

impl SprintPlan {
    pub fn canonicalize(&mut self) -> Vec<SprintCanonicalizationWarning> {
        let mut warnings = Vec::new();
        self.normalize_text_fields();
        if self.ends_at.is_some() && self.length.is_some() {
            warnings.push(SprintCanonicalizationWarning::LengthDiscardedInFavorOfEndsAt);
            self.length = None;
        }

        if let Some(capacity) = &mut self.capacity {
            capacity.canonicalize();
            if capacity.is_empty() {
                self.capacity = None;
            }
        }

        warnings
    }

    fn is_empty(&self) -> bool {
        self.label.is_none()
            && self.goal.is_none()
            && self.length.is_none()
            && self.ends_at.is_none()
            && self.starts_at.is_none()
            && self.capacity.is_none()
            && self.overdue_after.is_none()
            && self.notes.is_none()
    }

    fn normalize_text_fields(&mut self) {
        Self::normalize_text(&mut self.label);
        Self::normalize_text(&mut self.goal);
        Self::normalize_text(&mut self.length);
        Self::normalize_text(&mut self.ends_at);
        Self::normalize_text(&mut self.starts_at);
        Self::normalize_text(&mut self.overdue_after);
        Self::normalize_text(&mut self.notes);
    }

    fn normalize_text(target: &mut Option<String>) {
        if let Some(value) = target.take() {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                *target = None;
            } else if trimmed.len() == value.len() {
                *target = Some(value);
            } else {
                *target = Some(trimmed.to_string());
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCapacity {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hours: Option<u32>,
}

impl SprintCapacity {
    fn canonicalize(&mut self) {
        if self.points == Some(0) {
            self.points = None;
        }
        if self.hours == Some(0) {
            self.hours = None;
        }
    }

    fn is_empty(&self) -> bool {
        self.points.is_none() && self.hours.is_none()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintActual {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub closed_at: Option<String>,
}

impl SprintActual {
    fn canonicalize(&mut self) {
        Self::normalize_text(&mut self.started_at);
        Self::normalize_text(&mut self.closed_at);
    }

    fn is_empty(&self) -> bool {
        self.started_at.is_none() && self.closed_at.is_none()
    }

    fn normalize_text(target: &mut Option<String>) {
        if let Some(value) = target.take() {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                *target = None;
            } else if trimmed.len() == value.len() {
                *target = Some(value);
            } else {
                *target = Some(trimmed.to_string());
            }
        }
    }
}

pub type SprintHistoryEntry = TaskChangeLogEntry;
pub type SprintHistoryChange = TaskChange;
