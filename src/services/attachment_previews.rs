//! Attachment previews for the web UI: a hash -> file index, downscaled
//! thumbnails and basic file info. Everything is computed on demand and kept
//! only in memory (see [`crate::services::memory_cache`]); nothing is written
//! next to the attachments.

use crate::services::memory_cache::{MemoryCache, Sweep, register_for_sweeping};
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Entries untouched this long are dropped by the background sweeper.
const IDLE: Duration = Duration::from_secs(60 * 60);
const THUMBNAIL_BUDGET: usize = 64 * 1024 * 1024;
const INFO_BUDGET: usize = 2 * 1024 * 1024;
const INDEX_BUDGET: usize = 8 * 1024 * 1024;
/// A missing hash only triggers a directory rescan this often.
const RESCAN_AFTER: Duration = Duration::from_secs(1);
/// Larger files are never decoded for previews.
const MAX_DECODE_BYTES: u64 = 64 * 1024 * 1024;
/// Thumbnail widths the UI may ask for; requests snap up to one of these so
/// the cache holds a few variants per image, not one per pixel width.
pub const THUMBNAIL_WIDTHS: [u32; 5] = [96, 240, 480, 960, 1600];

fn cache<K, V>(
    slot: &'static OnceLock<Arc<MemoryCache<K, V>>>,
    budget: usize,
) -> &'static MemoryCache<K, V>
where
    K: Eq + std::hash::Hash + Clone + Send + 'static,
    V: Send + Sync + 'static,
{
    slot.get_or_init(|| {
        let cache = Arc::new(MemoryCache::new(budget, IDLE));
        let sweep: Arc<dyn Sweep> = cache.clone();
        register_for_sweeping(&sweep);
        cache
    })
}

// ---------------------------------------------------------------------------
// Hash index
// ---------------------------------------------------------------------------

struct HashIndex {
    files: HashMap<String, PathBuf>,
    built: Instant,
}

fn index_cache() -> &'static MemoryCache<PathBuf, HashIndex> {
    static SLOT: OnceLock<Arc<MemoryCache<PathBuf, HashIndex>>> = OnceLock::new();
    cache(&SLOT, INDEX_BUDGET)
}

/// The 32-hex hash tag of a stored attachment name (`name.<hash>.ext`, or the
/// older `name-<hash>.ext`), and whether it used the preferred dot form.
fn hash_tag_of(path: &Path) -> Option<(String, bool)> {
    let stem = path.file_stem().and_then(OsStr::to_str)?;
    for (separator, dot) in [('.', true), ('-', false)] {
        if let Some((_, tag)) = stem.rsplit_once(separator)
            && is_hash(tag)
        {
            return Some((tag.to_ascii_lowercase(), dot));
        }
    }
    None
}

fn is_hash(tag: &str) -> bool {
    tag.len() == 32 && tag.bytes().all(|b| b.is_ascii_hexdigit())
}

fn build_index(root: &Path) -> HashIndex {
    let mut files: HashMap<String, (PathBuf, bool)> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some((tag, dot)) = hash_tag_of(&path) else {
                continue;
            };
            // Prefer the current `name.<hash>.ext` form over the legacy dash form.
            match files.get(&tag) {
                Some((_, true)) if !dot => {}
                _ => {
                    files.insert(tag, (path, dot));
                }
            }
        }
    }
    HashIndex {
        files: files
            .into_iter()
            .map(|(tag, (path, _))| (tag, path))
            .collect(),
        built: Instant::now(),
    }
}

/// Forget the index for a store after this process wrote to it, so the next
/// lookup sees the new file at once.
pub fn forget_index(root: &Path) {
    index_cache().remove(&root.to_path_buf());
}

/// The stored file for a content hash, via an in-memory index of the store.
/// A miss rescans the directory (at most once a second), so files added by
/// git or another process are found too.
pub fn find_by_hash(root: &Path, hash: &str) -> Option<PathBuf> {
    if !is_hash(hash) {
        return None;
    }
    let hash = hash.to_ascii_lowercase();
    let key = root.to_path_buf();
    let cached = index_cache().get(&key);
    if let Some(index) = &cached
        && let Some(path) = index.files.get(&hash)
        && path.is_file()
    {
        return Some(path.clone());
    }
    if cached.as_ref().is_some_and(|index| {
        index.built.elapsed() < RESCAN_AFTER && !index.files.contains_key(&hash)
    }) {
        return None;
    }
    let index = build_index(root);
    let cost = 64 + index.files.len() * 160;
    let index = index_cache().insert(key, index, cost);
    index
        .files
        .get(&hash)
        .filter(|path| path.is_file())
        .cloned()
}

// ---------------------------------------------------------------------------
// Thumbnails
// ---------------------------------------------------------------------------

pub struct Thumbnail {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// `None` = serve the original (already small enough, or not decodable).
type ThumbnailEntry = Option<Arc<Thumbnail>>;

fn thumbnail_cache() -> &'static MemoryCache<(String, u32), ThumbnailEntry> {
    static SLOT: OnceLock<Arc<MemoryCache<(String, u32), ThumbnailEntry>>> = OnceLock::new();
    cache(&SLOT, THUMBNAIL_BUDGET)
}

/// The supported thumbnail width for a requested width.
pub fn snap_width(requested: u32) -> u32 {
    THUMBNAIL_WIDTHS
        .iter()
        .copied()
        .find(|width| *width >= requested)
        .unwrap_or(THUMBNAIL_WIDTHS[THUMBNAIL_WIDTHS.len() - 1])
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// A downscaled copy of the image at `path`, or `None` when the original
/// should be served as is: it already fits, it is vector art, an animation,
/// a format the server does not decode (AVIF, HEIC), or not an image at all.
/// TIFF, which browsers cannot show, is always converted.
pub fn thumbnail(path: &Path, hash: &str, requested_width: u32) -> Option<Arc<Thumbnail>> {
    let width = snap_width(requested_width);
    let key = (hash.to_ascii_lowercase(), width);
    let entry = match thumbnail_cache().get(&key) {
        Some(entry) => entry,
        None => {
            let made = make_thumbnail(path, width).map(Arc::new);
            let cost = 64 + made.as_ref().map_or(0, |thumb| thumb.bytes.len());
            thumbnail_cache().insert(key, made, cost)
        }
    };
    (*entry).clone()
}

fn make_thumbnail(path: &Path, width: u32) -> Option<Thumbnail> {
    let ext = extension(path);
    let always = matches!(ext.as_str(), "tif" | "tiff");
    let size = std::fs::metadata(path).ok()?.len();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "ico" | "tif" | "tiff" => {}
        // Keep small GIFs animated; only big ones get a still preview.
        "gif" if size > 1024 * 1024 => {}
        _ => return None,
    }
    if size > MAX_DECODE_BYTES {
        return None;
    }
    let path = path.to_path_buf();
    // Decoders are hardened, but a preview must never take the server down.
    std::panic::catch_unwind(move || downscale(&path, width, always))
        .ok()
        .flatten()
}

fn downscale(path: &Path, width: u32, always: bool) -> Option<Thumbnail> {
    let mut reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }

    let max_height = width.saturating_mul(4);
    if !always && image.width() <= width && image.height() <= max_height {
        return None;
    }
    if image.width() > width || image.height() > max_height {
        image = image.thumbnail(width, max_height);
    }
    encode(&image)
}

fn has_transparency(image: &DynamicImage) -> bool {
    image.color().has_alpha() && image.to_rgba8().pixels().any(|pixel| pixel.0[3] < 255)
}

fn encode(image: &DynamicImage) -> Option<Thumbnail> {
    let mut bytes = Vec::new();
    if has_transparency(image) {
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .ok()?;
        return Some(Thumbnail {
            bytes,
            content_type: "image/png",
        });
    }
    let rgb = image.to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82)
        .encode_image(&rgb)
        .ok()?;
    Some(Thumbnail {
        bytes,
        content_type: "image/jpeg",
    })
}

// ---------------------------------------------------------------------------
// File info
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentFileInfo {
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn info_cache() -> &'static MemoryCache<String, AttachmentFileInfo> {
    static SLOT: OnceLock<Arc<MemoryCache<String, AttachmentFileInfo>>> = OnceLock::new();
    cache(&SLOT, INFO_BUDGET)
}

/// Size and, for raster images, pixel dimensions (read from the header only).
pub fn file_info(path: &Path, hash: &str) -> Option<AttachmentFileInfo> {
    let key = hash.to_ascii_lowercase();
    if let Some(info) = info_cache().get(&key) {
        return Some((*info).clone());
    }
    let size = std::fs::metadata(path).ok()?.len();
    let dimensions = {
        let path = path.to_path_buf();
        std::panic::catch_unwind(move || {
            ImageReader::open(&path)
                .ok()?
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()
        })
        .ok()
        .flatten()
    };
    let info = AttachmentFileInfo {
        size,
        width: dimensions.map(|(width, _)| width),
        height: dimensions.map(|(_, height)| height),
    };
    info_cache().insert(key, info.clone(), 64);
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_image(
        dir: &Path,
        name: &str,
        image: &DynamicImage,
        format: image::ImageFormat,
    ) -> PathBuf {
        let path = dir.join(name);
        image.save_with_format(&path, format).unwrap();
        path
    }

    fn gradient(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }))
    }

    #[test]
    fn snaps_requested_widths_up() {
        assert_eq!(snap_width(1), 96);
        assert_eq!(snap_width(96), 96);
        assert_eq!(snap_width(300), 480);
        assert_eq!(snap_width(5000), 1600);
    }

    #[test]
    fn downscales_large_photos_and_leaves_small_ones() {
        let dir = tempfile::tempdir().unwrap();
        let hash = "a".repeat(32);
        let big = write_image(
            dir.path(),
            &format!("photo.{hash}.jpg"),
            &gradient(1200, 800),
            image::ImageFormat::Jpeg,
        );
        let thumb = thumbnail(&big, &hash, 300).expect("large photo gets a thumbnail");
        assert_eq!(thumb.content_type, "image/jpeg");
        let decoded = image::load_from_memory(&thumb.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (480, 320));

        let small_hash = "b".repeat(32);
        let small = write_image(
            dir.path(),
            &format!("icon.{small_hash}.png"),
            &gradient(16, 16),
            image::ImageFormat::Png,
        );
        assert!(thumbnail(&small, &small_hash, 96).is_none(), "fits already");
    }

    #[test]
    fn keeps_transparency_and_converts_tiff() {
        let dir = tempfile::tempdir().unwrap();
        let hash = "c".repeat(32);
        let logo = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(600, 600, |x, _| {
            image::Rgba([0, 0, 255, if x < 300 { 0 } else { 255 }])
        }));
        let path = write_image(
            dir.path(),
            &format!("logo.{hash}.png"),
            &logo,
            image::ImageFormat::Png,
        );
        assert_eq!(
            thumbnail(&path, &hash, 240).unwrap().content_type,
            "image/png"
        );

        let tiff_hash = "d".repeat(32);
        let tiff = write_image(
            dir.path(),
            &format!("scan.{tiff_hash}.tiff"),
            &gradient(64, 64),
            image::ImageFormat::Tiff,
        );
        let converted = thumbnail(&tiff, &tiff_hash, 96).expect("tiff is always converted");
        assert_eq!(converted.content_type, "image/jpeg");
    }

    #[test]
    fn non_images_and_broken_files_fall_back_to_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let hash = "e".repeat(32);
        let broken = dir.path().join(format!("broken.{hash}.png"));
        std::fs::write(&broken, b"not a png").unwrap();
        assert!(thumbnail(&broken, &hash, 96).is_none());
        let svg_hash = "f".repeat(32);
        let svg = dir.path().join(format!("art.{svg_hash}.svg"));
        std::fs::write(&svg, b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert!(thumbnail(&svg, &svg_hash, 96).is_none());
    }

    #[test]
    fn index_finds_files_and_notices_new_ones() {
        let dir = tempfile::tempdir().unwrap();
        let first = "1".repeat(32);
        std::fs::write(dir.path().join(format!("a.{first}.txt")), b"a").unwrap();
        assert!(find_by_hash(dir.path(), &first).is_some());
        assert!(find_by_hash(dir.path(), "not-a-hash").is_none());

        // Added behind the server's back: found after the rescan delay.
        let second = "2".repeat(32);
        std::fs::write(dir.path().join(format!("b-{second}.txt")), b"b").unwrap();
        std::thread::sleep(RESCAN_AFTER + Duration::from_millis(50));
        assert_eq!(
            find_by_hash(dir.path(), &second)
                .unwrap()
                .file_name()
                .unwrap(),
            format!("b-{second}.txt").as_str()
        );

        // Removed files are not returned from a stale index.
        std::fs::remove_file(dir.path().join(format!("a.{first}.txt"))).unwrap();
        assert!(find_by_hash(dir.path(), &first).is_none());
    }

    #[test]
    fn info_reports_size_and_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let hash = "9".repeat(32);
        let path = write_image(
            dir.path(),
            &format!("p.{hash}.png"),
            &gradient(30, 20),
            image::ImageFormat::Png,
        );
        let info = file_info(&path, &hash).unwrap();
        assert_eq!((info.width, info.height), (Some(30), Some(20)));
        assert!(info.size > 0);
        let text_hash = "8".repeat(32);
        let text = dir.path().join(format!("t.{text_hash}.txt"));
        std::fs::write(&text, b"hello").unwrap();
        assert_eq!(
            file_info(&text, &text_hash).unwrap(),
            AttachmentFileInfo {
                size: 5,
                width: None,
                height: None
            }
        );
    }
}
