//! Repository branding for the web UI (DEV-218): the default colour theme,
//! a display name, and logo/icon images stored in the repository itself.
//!
//! Image settings are paths relative to the workspace root (the folder that
//! holds the tasks directory), e.g. `assets/app-icon.svg` or
//! `.tasks/@branding/logo.png`. They are only ever served from inside that
//! folder and only when the file really is an image, so a shared config can
//! never expose other files.

use crate::config::branding::{IMAGE_EXTENSIONS, validate_image_path};
use std::path::{Path, PathBuf};

/// Largest branding image the server will read.
pub const MAX_BRANDING_BYTES: u64 = 2 * 1024 * 1024;

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

/// The folder branding paths are relative to: the parent of the tasks
/// directory (the repository root in a normal setup).
pub fn workspace_root(tasks_dir: &Path) -> PathBuf {
    tasks_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| tasks_dir.to_path_buf())
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
    let root = root.canonicalize().map_err(|_| BrandingFileError::Missing)?;
    let path = root.join(relative).canonicalize().map_err(|_| BrandingFileError::Missing)?;
    // Symlinks may point anywhere; only files really inside the workspace count.
    if !path.starts_with(&root) || !path.is_file() {
        return Err(BrandingFileError::Missing);
    }
    let size = std::fs::metadata(&path).map_err(|_| BrandingFileError::Missing)?.len();
    if size > MAX_BRANDING_BYTES {
        return Err(BrandingFileError::TooLarge);
    }
    let bytes = std::fs::read(&path).map_err(|_| BrandingFileError::Missing)?;
    if crate::services::attachment_metadata::sniff_image_format(&bytes).is_none() {
        return Err(BrandingFileError::NotAnImage);
    }
    Ok((path, bytes))
}

/// Folders searched for the repository's own icon when `ui.icon` is not set,
/// root first, then the usual web-project places (DEV-345).
const ICON_FOLDERS: &[&str] = &["", "public", "static", "assets", "app", "src", "src/app", "web"];

/// File names tried in each of [`ICON_FOLDERS`], vector images first.
const ICON_NAMES: &[&str] = &["favicon.svg", "favicon.ico", "favicon.png", "icon.svg", "icon.png"];

/// The repository's own icon (e.g. `public/favicon.svg`), used for the browser
/// tab when no `ui.icon` is configured.
pub fn find_icon(root: &Path) -> Option<String> {
    ICON_FOLDERS
        .iter()
        .flat_map(|folder| ICON_NAMES.iter().map(move |name| if folder.is_empty() { (*name).to_string() } else { format!("{folder}/{name}") }))
        .find(|relative| read_image(root, relative).is_ok())
}

/// The repository's folder name, shown in the browser tab without `ui.name`.
pub fn repository_name(root: &Path) -> Option<String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    root.file_name().map(|name| name.to_string_lossy().into_owned())
}

/// A safe file name for an uploaded branding image, e.g. `logo-dark.svg`.
pub fn upload_file_name(image: BrandingImage, original: &str) -> Result<String, String> {
    let extension = Path::new(original).extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("'{original}' is not an image (expected {})", IMAGE_EXTENSIONS.join(", ")));
    }
    Ok(format!("{}.{extension}", image.slug()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(read_image(&root, "assets/fake.png"), Err(BrandingFileError::NotAnImage));
        assert_eq!(read_image(&root, "assets/missing.png"), Err(BrandingFileError::Missing));
        assert!(matches!(read_image(&root, "../secret.png"), Err(BrandingFileError::Invalid(_))));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path().join("secret.png"), root.join("assets/link.png")).unwrap();
            assert_eq!(read_image(&root, "assets/link.png"), Err(BrandingFileError::Missing));
        }
    }

    #[test]
    fn finds_the_repository_icon() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("my-app");
        std::fs::create_dir_all(root.join("public")).unwrap();
        assert_eq!(find_icon(&root), None);
        assert_eq!(repository_name(&root).as_deref(), Some("my-app"));

        std::fs::write(root.join("public/favicon.ico"), b"\0\0\x01\0\x01\0\x10\x10\0\0\x01\0\x20\0").unwrap();
        assert_eq!(find_icon(&root).as_deref(), Some("public/favicon.ico"));
        std::fs::write(root.join("public/favicon.svg"), b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert_eq!(find_icon(&root).as_deref(), Some("public/favicon.svg"));
        // A file that only claims to be an icon is skipped.
        std::fs::write(root.join("favicon.png"), b"not a png").unwrap();
        assert_eq!(find_icon(&root).as_deref(), Some("public/favicon.svg"));
    }

    #[test]
    fn names_uploads_by_slot() {
        assert_eq!(upload_file_name(BrandingImage::LogoDark, "Brand Mark.SVG").unwrap(), "logo-dark.svg");
        assert!(upload_file_name(BrandingImage::Icon, "icon.exe").is_err());
    }
}
