//! Repository branding for the web UI (DEV-218): the default colour theme,
//! a display name, and logo/icon images stored in the repository itself.
//!
//! Image settings are paths relative to the workspace root (the folder that
//! holds the tasks directory), e.g. `assets/app-icon.svg` or
//! `.tasks/@branding/logo.png`. They are only ever served from inside that
//! folder and only when the file really is an image, so a shared config can
//! never expose other files.

use std::path::{Component, Path, PathBuf};

/// Skins the web UI ships and their colour palettes, classic then playful
/// (keep in sync with `THEMES` in `view/utils/themes.ts`;
/// `view/__tests__/themes.spec.ts` checks this). The first palette is the
/// skin's default; an empty list means one fixed palette. `ui.theme` is
/// written `skin` or `skin:palette`.
pub const UI_THEMES: &[(&str, &[&str])] = &[
    (
        "classic",
        &["lotar", "ocean", "forest", "ember", "violet", "rose"],
    ),
    ("contrast", &[]),
    ("terminal", &["amber", "green", "red", "cyan", "white"]),
    ("matrix", &["green", "red", "blue"]),
    ("synthwave", &["sunset", "outrun", "ice"]),
    ("blueprint", &["blue", "green", "sepia"]),
    ("paper", &["blue", "black", "red", "green"]),
    ("retro", &["teal", "plum", "slate", "desert"]),
    ("candy", &["bubblegum", "mint", "grape"]),
];

/// Theme ids from before palettes (DEV-218/219), still accepted; they map to
/// classic palettes (`LEGACY_THEME_ALIASES` in `view/utils/themes.ts`).
pub const UI_THEME_ALIASES: &[(&str, &str)] = &[
    ("lotar", "classic"),
    ("ocean", "classic:ocean"),
    ("forest", "classic:forest"),
    ("ember", "classic:ember"),
    ("violet", "classic:violet"),
    ("rose", "classic:rose"),
];

/// `ui.theme` value selecting the repository's own skin file.
pub const CUSTOM_UI_THEME: &str = "custom";

/// Check a `ui.theme` value: `skin`, `skin:palette` with a known palette, a
/// legacy id, or `custom`. Surrounding whitespace is ignored.
pub fn check_ui_theme(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value == CUSTOM_UI_THEME {
        return Ok(());
    }
    let value = UI_THEME_ALIASES
        .iter()
        .find(|(alias, _)| *alias == value)
        .map_or(value, |(_, target)| *target);
    let (skin, palette) = match value.split_once(':') {
        Some((skin, palette)) => (skin, Some(palette)),
        None => (value, None),
    };
    let Some((_, palettes)) = UI_THEMES.iter().find(|(id, _)| *id == skin) else {
        let skins: Vec<&str> = UI_THEMES.iter().map(|(id, _)| *id).collect();
        return Err(format!(
            "Unknown theme '{value}' (expected a theme such as classic or terminal:red, or {CUSTOM_UI_THEME}; themes: {})",
            skins.join(", ")
        ));
    };
    match palette {
        None => Ok(()),
        Some(palette) if palettes.contains(&palette) => Ok(()),
        Some(palette) if palettes.is_empty() => Err(format!(
            "Theme '{skin}' has no colour palettes (got '{palette}')"
        )),
        Some(palette) => Err(format!(
            "Unknown palette '{palette}' for theme '{skin}' (expected one of: {})",
            palettes.join(", ")
        )),
    }
}

/// Whether `value` is an accepted `ui.theme` (see [`check_ui_theme`]).
pub fn is_valid_ui_theme(value: &str) -> bool {
    check_ui_theme(value).is_ok()
}

/// Longest display name shown next to the logo.
pub const MAX_UI_NAME_CHARS: usize = 40;

/// Largest branding image the server will read.
pub const MAX_BRANDING_BYTES: u64 = 2 * 1024 * 1024;

/// Extensions accepted for logos and icons.
pub const IMAGE_EXTENSIONS: &[&str] = &["svg", "png", "jpg", "jpeg", "webp", "gif", "ico", "avif"];

/// Folder (inside the tasks directory) where uploaded branding images go.
pub const UPLOAD_DIR: &str = "@branding";

/// Which branding image a request is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrandingImage {
    Logo,
    LogoDark,
    Icon,
}

impl BrandingImage {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "logo" => Some(Self::Logo),
            "logo-dark" => Some(Self::LogoDark),
            "icon" => Some(Self::Icon),
            _ => None,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::LogoDark => "logo-dark",
            Self::Icon => "icon",
        }
    }

    /// The config field that names this image.
    pub fn field(self) -> &'static str {
        match self {
            Self::Logo => "ui_logo",
            Self::LogoDark => "ui_logo_dark",
            Self::Icon => "ui_icon",
        }
    }

    pub fn configured(self, config: &crate::config::types::ResolvedConfig) -> Option<&str> {
        match self {
            Self::Logo => config.ui.logo.as_deref(),
            Self::LogoDark => config.ui.logo_dark.as_deref(),
            Self::Icon => config.ui.icon.as_deref(),
        }
    }
}

/// Validate a `ui_*` config value. Empty values clear a setting and are
/// always accepted.
pub fn validate_ui_field(field: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    match field {
        "ui_theme" => check_ui_theme(value),
        "ui_name" => {
            if value.chars().count() > MAX_UI_NAME_CHARS {
                Err(format!(
                    "ui.name is too long (at most {MAX_UI_NAME_CHARS} characters)"
                ))
            } else if value.chars().any(char::is_control) {
                Err("ui.name cannot contain control characters".to_string())
            } else {
                Ok(())
            }
        }
        "ui_logo" | "ui_logo_dark" | "ui_icon" => validate_image_path(value),
        _ => Ok(()),
    }
}

/// A branding image path must be relative, stay inside the workspace, and
/// name an image file.
pub fn validate_image_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute() || value.starts_with('~') {
        return Err(format!(
            "'{value}' must be a path relative to the repository (for example assets/logo.svg)"
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("'{value}' must stay inside the repository"));
    }
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!(
            "'{value}' is not an image (expected {})",
            IMAGE_EXTENSIONS.join(", ")
        ));
    }
    Ok(())
}

/// The folder branding paths are relative to: the parent of the tasks
/// directory (the repository root in a normal setup).
pub fn workspace_root(tasks_dir: &Path) -> PathBuf {
    tasks_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| tasks_dir.to_path_buf())
}

/// Why a configured image cannot be served.
#[derive(Debug, PartialEq, Eq)]
pub enum BrandingFileError {
    /// The setting is invalid (see [`validate_image_path`]).
    Invalid(String),
    /// No such file, or it resolves (e.g. via a symlink) outside the workspace.
    Missing,
    /// Larger than [`MAX_BRANDING_BYTES`].
    TooLarge,
    /// The file is not the image its name claims.
    NotAnImage,
}

/// Read a configured branding image from inside `root`.
pub fn read_image(root: &Path, relative: &str) -> Result<(PathBuf, Vec<u8>), BrandingFileError> {
    validate_image_path(relative).map_err(BrandingFileError::Invalid)?;
    let root = root
        .canonicalize()
        .map_err(|_| BrandingFileError::Missing)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| BrandingFileError::Missing)?;
    // Symlinks may point anywhere; only files really inside the workspace count.
    if !path.starts_with(&root) || !path.is_file() {
        return Err(BrandingFileError::Missing);
    }
    let size = std::fs::metadata(&path)
        .map_err(|_| BrandingFileError::Missing)?
        .len();
    if size > MAX_BRANDING_BYTES {
        return Err(BrandingFileError::TooLarge);
    }
    let bytes = std::fs::read(&path).map_err(|_| BrandingFileError::Missing)?;
    if crate::services::attachment_metadata::sniff_image_format(&bytes).is_none() {
        return Err(BrandingFileError::NotAnImage);
    }
    Ok((path, bytes))
}

/// A safe file name for an uploaded branding image, e.g. `logo-dark.svg`.
pub fn upload_file_name(image: BrandingImage, original: &str) -> Result<String, String> {
    let extension = Path::new(original)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!(
            "'{original}' is not an image (expected {})",
            IMAGE_EXTENSIONS.join(", ")
        ));
    }
    Ok(format!("{}.{extension}", image.slug()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_settings() {
        assert!(validate_ui_field("ui_theme", "").is_ok());
        assert!(validate_ui_field("ui_theme", "neon").is_err());
        assert!(validate_ui_field("ui_name", "Acme Tracker").is_ok());
        assert!(validate_ui_field("ui_name", &"x".repeat(41)).is_err());
        assert!(validate_ui_field("ui_logo", "assets/logo.svg").is_ok());
        assert!(validate_ui_field("ui_logo", "./assets/logo.PNG").is_ok());
        for bad in [
            "/etc/logo.png",
            "~/logo.png",
            "../logo.png",
            "assets/../../x.png",
            "assets/notes.txt",
            "assets/logo",
        ] {
            assert!(validate_ui_field("ui_icon", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validates_themes_and_palettes() {
        for ok in [
            "classic",
            "classic:lotar",
            "classic:ocean",
            "terminal",
            "terminal:red",
            " terminal:amber ",
            "contrast",
            "custom",
            // Legacy ids from before palettes.
            "lotar",
            "forest",
            "rose",
        ] {
            assert!(validate_ui_field("ui_theme", ok).is_ok(), "{ok}");
            assert!(is_valid_ui_theme(ok), "{ok}");
        }
        for bad in [
            "neon",
            "terminal:pink",
            "terminal:",
            "contrast:dark",
            "custom:red",
            "forest:ember",
            "classic:ocean:x",
            ":red",
        ] {
            assert!(validate_ui_field("ui_theme", bad).is_err(), "{bad}");
        }
        let err = check_ui_theme("terminal:pink").unwrap_err();
        assert!(err.contains("amber, green, red, cyan, white"), "{err}");
        let err = check_ui_theme("neon").unwrap_err();
        assert!(err.contains("classic, contrast, terminal"), "{err}");
    }

    #[test]
    fn theme_aliases_point_at_real_palettes() {
        for (alias, target) in UI_THEME_ALIASES {
            assert!(UI_THEMES.iter().all(|(skin, _)| skin != alias), "{alias}");
            assert!(is_valid_ui_theme(target), "{alias} -> {target}");
        }
        for (skin, palettes) in UI_THEMES {
            assert!(!skin.contains(':') && *skin != CUSTOM_UI_THEME, "{skin}");
            assert_ne!(palettes.len(), 1, "{skin}: a single palette should be none");
        }
    }

    #[test]
    fn reads_only_real_images_inside_the_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg'/>";
        std::fs::write(root.join("assets/logo.svg"), svg).unwrap();
        std::fs::write(root.join("assets/fake.png"), b"not a png").unwrap();
        std::fs::write(tmp.path().join("secret.png"), b"\x89PNG\r\n\x1a\nsecret").unwrap();

        let (_, bytes) = read_image(&root, "assets/logo.svg").unwrap();
        assert_eq!(bytes, svg);
        assert_eq!(
            read_image(&root, "assets/fake.png"),
            Err(BrandingFileError::NotAnImage)
        );
        assert_eq!(
            read_image(&root, "assets/missing.png"),
            Err(BrandingFileError::Missing)
        );
        assert!(matches!(
            read_image(&root, "../secret.png"),
            Err(BrandingFileError::Invalid(_))
        ));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path().join("secret.png"), root.join("assets/link.png"))
                .unwrap();
            assert_eq!(
                read_image(&root, "assets/link.png"),
                Err(BrandingFileError::Missing)
            );
        }
    }

    #[test]
    fn names_uploads_by_slot() {
        assert_eq!(
            upload_file_name(BrandingImage::LogoDark, "Brand Mark.SVG").unwrap(),
            "logo-dark.svg"
        );
        assert!(upload_file_name(BrandingImage::Icon, "icon.exe").is_err());
    }
}
