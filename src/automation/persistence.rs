use crate::automation::types::{AutomationFile, AutomationRule, AutomationRuleSet};
use crate::errors::{LoTaRError, LoTaRResult};
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_global_automation(tasks_dir: &Path) -> LoTaRResult<Option<AutomationFile>> {
    let path = crate::utils::paths::global_automation_path(tasks_dir);
    load_automation_from_path(&path)
}

pub fn load_project_automation(
    tasks_dir: &Path,
    project: &str,
) -> LoTaRResult<Option<AutomationFile>> {
    let path = crate::utils::paths::project_automation_path(tasks_dir, project);
    load_automation_from_path(&path)
}

pub fn load_home_automation() -> LoTaRResult<Option<AutomationFile>> {
    if std::env::var("RUST_TEST_THREADS").is_ok()
        || std::env::var("LOTAR_TEST_MODE")
            .map(|v| v == "1")
            .unwrap_or(false)
        || std::env::var("LOTAR_IGNORE_HOME_CONFIG")
            .map(|v| v == "1")
            .unwrap_or(false)
    {
        return Ok(None);
    }

    let path = resolve_home_automation_path()?;
    load_automation_from_path(&path)
}

pub fn save_global_automation(tasks_dir: &Path, file: &AutomationFile) -> LoTaRResult<()> {
    let path = crate::utils::paths::global_automation_path(tasks_dir);
    write_automation_file(&path, file)
}

pub fn save_project_automation(
    tasks_dir: &Path,
    project: &str,
    file: &AutomationFile,
) -> LoTaRResult<()> {
    let path = crate::utils::paths::project_automation_path(tasks_dir, project);
    write_automation_file(&path, file)
}

pub fn save_home_automation(file: &AutomationFile) -> LoTaRResult<()> {
    let path = resolve_home_automation_path()?;
    write_automation_file(&path, file)
}

pub fn to_canonical_yaml(file: &AutomationFile) -> LoTaRResult<String> {
    #[derive(serde::Serialize)]
    struct AutomationFileOutput {
        automation: AutomationRulesOutput,
    }

    #[derive(serde::Serialize)]
    struct AutomationRulesOutput {
        #[serde(skip_serializing_if = "Option::is_none")]
        max_iterations: Option<u32>,
        rules: Vec<AutomationRule>,
    }

    let rules = file.automation.rules().to_vec();
    let output = AutomationFileOutput {
        automation: AutomationRulesOutput {
            max_iterations: file.automation.max_iterations(),
            rules,
        },
    };
    serde_yaml_ng::to_string(&output).map_err(LoTaRError::from)
}

/// Whether two automation files carry the same COMPLETE effective rules
/// (DEV-102 no-op boundary), compared under the same projection
/// [`to_canonical_yaml`] serializes: the ordered rule list plus
/// `max_iterations`.
///
/// Parse-normalized spellings compare equal — a bare rule list versus the
/// `rules:` map form, serde aliases such as `triggers`/`when`, scalar/list
/// condition shorthands, and scalar/singleton-list actions, except the
/// variant-sensitive assignee placeholders, so equivalent syntax does not rewrite
/// the stored file. Anything that changes matching behavior does not
/// compare equal: rule content, rule ORDER, `max_iterations`, cooldowns,
/// names, conditions, and actions are all part of the comparison.
///
/// Comments and unknown extensions in the stored raw file are invisible
/// here by design: a semantically equal submission preserves them (and the
/// file's bytes/mtime) instead of canonical-reformatting the file; removing
/// such raw-only content requires editing the file, not `set`.
pub fn files_effectively_equal(a: &AutomationFile, b: &AutomationFile) -> bool {
    a.automation.rules() == b.automation.rules()
        && a.automation.max_iterations() == b.automation.max_iterations()
}

fn load_automation_from_path(path: &Path) -> LoTaRResult<Option<AutomationFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    let parsed = serde_yaml_ng::from_str::<AutomationFile>(&content).map_err(LoTaRError::from)?;
    Ok(Some(parsed))
}

fn write_automation_file(path: &Path, file: &AutomationFile) -> LoTaRResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = to_canonical_yaml(file)?;
    fs::write(path, payload)?;
    Ok(())
}

fn resolve_home_automation_path() -> LoTaRResult<PathBuf> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        LoTaRError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Home directory not found",
        ))
    })?;
    let legacy_file = home_dir.join(".lotar");
    if legacy_file.is_dir() {
        return Ok(legacy_file.join("automation.yml"));
    }
    Ok(home_dir.join(".lotar.automation.yml"))
}

pub fn empty_file() -> AutomationFile {
    AutomationFile {
        automation: AutomationRuleSet::Rules {
            rules: Vec::new(),
            max_iterations: None,
        },
    }
}
