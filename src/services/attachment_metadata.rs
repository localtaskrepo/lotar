//! Removes privacy-sensitive metadata (GPS position, device serials, capture
//! times, comments, editing history) from uploaded images.
//!
//! Only container bytes are edited: pixel data is copied verbatim and never
//! re-encoded, so there is no quality loss. Data that changes how an image
//! looks (ICC profiles, gamma, colour transforms, JPEG orientation) is kept.
//!
//! Supported: JPEG, PNG and WebP. HEIC/AVIF (ISO-BMFF `iloc` offsets) and TIFF
//! (offset-linked IFDs) would need a full container rewrite, so they are left
//! untouched, as are GIF, BMP, ICO, SVG and non-images.

/// Bytes with privacy-sensitive metadata removed, or None when nothing was
/// removed (unsupported format, already clean, or the container could not be
/// parsed safely — never fail an upload because of this).
///
/// Dispatch uses the sniffed content, so a misnamed file (a `.png` holding
/// JPEG data) is still cleaned.
pub fn strip_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    let out = match sniff_image_format(bytes)? {
        "jpeg" => strip_jpeg(bytes)?,
        "png" => strip_png(bytes)?,
        "webp" => strip_webp(bytes)?,
        _ => return None,
    };
    (out != bytes).then_some(out)
}

/// The image format the bytes actually contain, from magic numbers:
/// "png", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "avif", "heic", "svg",
/// or None when the bytes are not a recognised image.
pub fn sniff_image_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(PNG_SIGNATURE) {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP".as_slice()) {
        return Some("webp");
    }
    if is_bmp(bytes) {
        return Some("bmp");
    }
    // ICO: reserved 0, type 1, at least one image.
    if bytes.starts_with(&[0, 0, 1, 0]) && bytes.get(4..6).is_some_and(|n| n != [0, 0]) {
        return Some("ico");
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some("tiff");
    }
    if let Some(format) = sniff_iso_bmff(bytes) {
        return Some(format);
    }
    looks_like_svg(bytes).then_some("svg")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ByteOrder {
    Little,
    Big,
}

impl ByteOrder {
    fn read_u16(self, bytes: &[u8], at: usize) -> Option<u16> {
        let raw: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(match self {
            Self::Little => u16::from_le_bytes(raw),
            Self::Big => u16::from_be_bytes(raw),
        })
    }

    fn read_u32(self, bytes: &[u8], at: usize) -> Option<u32> {
        let raw: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(match self {
            Self::Little => u32::from_le_bytes(raw),
            Self::Big => u32::from_be_bytes(raw),
        })
    }

    fn u16_bytes(self, value: u16) -> [u8; 2] {
        match self {
            Self::Little => value.to_le_bytes(),
            Self::Big => value.to_be_bytes(),
        }
    }

    fn u32_bytes(self, value: u32) -> [u8; 4] {
        match self {
            Self::Little => value.to_le_bytes(),
            Self::Big => value.to_be_bytes(),
        }
    }

    fn tiff_marker(self) -> &'static [u8; 2] {
        match self {
            Self::Little => b"II",
            Self::Big => b"MM",
        }
    }
}

// ---------------------------------------------------------------- JPEG

const EXIF_HEADER: &[u8] = b"Exif\0\0";
const TIFF_TAG_ORIENTATION: u16 = 0x0112;
const TIFF_TYPE_SHORT: u16 = 3;

/// Walks marker segments up to SOS, dropping APP1 (Exif/XMP), APP13 (IPTC),
/// other vendor APPn segments and COM; keeps APP0 (JFIF), APP2 (ICC/MPF),
/// APP14 (Adobe colour transform) and all non-APP markers. Everything from
/// SOS onward is copied verbatim.
fn strip_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut pos = 2;
    let mut first_app1_at = None;
    let mut seen_exif = false;
    let mut orientation = None;

    loop {
        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        let start = pos;
        // Markers may be preceded by any number of 0xFF fill bytes.
        while bytes.get(pos) == Some(&0xFF) {
            pos += 1;
        }
        let marker = *bytes.get(pos)?;
        pos += 1;
        match marker {
            // SOS (or a premature EOI): the rest is entropy-coded data.
            0xDA | 0xD9 => {
                out.extend_from_slice(bytes.get(start..)?);
                break;
            }
            // Standalone markers without a length field.
            0x01 | 0xD0..=0xD7 => {
                out.extend_from_slice(bytes.get(start..pos)?);
                continue;
            }
            0x00 | 0xD8 => return None,
            _ => {}
        }
        let len = usize::from(ByteOrder::Big.read_u16(bytes, pos)?);
        if len < 2 {
            return None;
        }
        let end = pos.checked_add(len)?;
        let segment = bytes.get(start..end)?;
        let payload = bytes.get(pos + 2..end)?;
        pos = end;
        match marker {
            0xE1 => {
                first_app1_at.get_or_insert(out.len());
                if !seen_exif && let Some(tiff) = payload.strip_prefix(EXIF_HEADER) {
                    seen_exif = true;
                    orientation = exif_orientation(tiff);
                }
            }
            0xE0 | 0xE2 | 0xEE => out.extend_from_slice(segment),
            0xE3..=0xED | 0xEF | 0xFE => {}
            _ => out.extend_from_slice(segment),
        }
    }

    if let (Some((order, value)), Some(at)) = (orientation, first_app1_at)
        && value != 1
    {
        out.splice(at..at, minimal_orientation_exif(order, value));
    }
    Some(out)
}

/// Reads the IFD0 Orientation tag from an Exif TIFF block. Any parse problem
/// (or an out-of-range value) yields None, i.e. "unknown".
fn exif_orientation(tiff: &[u8]) -> Option<(ByteOrder, u16)> {
    let order = match tiff.get(..2)? {
        b"II" => ByteOrder::Little,
        b"MM" => ByteOrder::Big,
        _ => return None,
    };
    if order.read_u16(tiff, 2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(order.read_u32(tiff, 4)?).ok()?;
    let count = usize::from(order.read_u16(tiff, ifd)?);
    for index in 0..count {
        let entry = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        if order.read_u16(tiff, entry)? != TIFF_TAG_ORIENTATION {
            continue;
        }
        let kind = order.read_u16(tiff, entry.checked_add(2)?)?;
        let values = order.read_u32(tiff, entry.checked_add(4)?)?;
        if kind != TIFF_TYPE_SHORT || values != 1 {
            return None;
        }
        let value = order.read_u16(tiff, entry.checked_add(8)?)?;
        return (1..=8).contains(&value).then_some((order, value));
    }
    None
}

/// An APP1 segment whose Exif holds nothing but IFD0 Orientation.
fn minimal_orientation_exif(order: ByteOrder, value: u16) -> Vec<u8> {
    let mut tiff = Vec::with_capacity(26);
    tiff.extend_from_slice(order.tiff_marker());
    tiff.extend_from_slice(&order.u16_bytes(42));
    tiff.extend_from_slice(&order.u32_bytes(8));
    tiff.extend_from_slice(&order.u16_bytes(1));
    tiff.extend_from_slice(&order.u16_bytes(TIFF_TAG_ORIENTATION));
    tiff.extend_from_slice(&order.u16_bytes(TIFF_TYPE_SHORT));
    tiff.extend_from_slice(&order.u32_bytes(1));
    tiff.extend_from_slice(&order.u16_bytes(value));
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&order.u32_bytes(0));

    // Fixed size (34), so the cast cannot truncate.
    let len = (2 + EXIF_HEADER.len() + tiff.len()) as u16;
    let mut segment = Vec::with_capacity(usize::from(len) + 2);
    segment.extend_from_slice(&[0xFF, 0xE1]);
    segment.extend_from_slice(&len.to_be_bytes());
    segment.extend_from_slice(EXIF_HEADER);
    segment.extend_from_slice(&tiff);
    segment
}

// ---------------------------------------------------------------- PNG

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_METADATA_CHUNKS: [&[u8; 4]; 5] = [b"eXIf", b"tEXt", b"zTXt", b"iTXt", b"tIME"];

/// Drops text, Exif and timestamp chunks. Chunks are copied whole, so CRCs
/// stay valid. Bytes after IEND are copied verbatim.
fn strip_png(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(PNG_SIGNATURE) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(PNG_SIGNATURE);
    let mut pos = PNG_SIGNATURE.len();
    loop {
        let len = ByteOrder::Big.read_u32(bytes, pos)?;
        if len > 0x7FFF_FFFF {
            return None;
        }
        let kind: [u8; 4] = bytes.get(pos + 4..pos + 8)?.try_into().ok()?;
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return None;
        }
        // length + type + data + CRC
        let end = pos
            .checked_add(12)?
            .checked_add(usize::try_from(len).ok()?)?;
        let chunk = bytes.get(pos..end)?;
        if !PNG_METADATA_CHUNKS.contains(&&kind) {
            out.extend_from_slice(chunk);
        }
        pos = end;
        if &kind == b"IEND" {
            out.extend_from_slice(bytes.get(pos..)?);
            return Some(out);
        }
    }
}

// ---------------------------------------------------------------- WebP

const VP8X_FLAG_EXIF: u8 = 0x08;
const VP8X_FLAG_XMP: u8 = 0x04;

/// Drops `EXIF` and `XMP ` chunks, clears their VP8X flags and rewrites the
/// RIFF size. Bytes after the RIFF container are copied verbatim.
fn strip_webp(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    let riff_end = usize::try_from(ByteOrder::Little.read_u32(bytes, 4)?)
        .ok()?
        .checked_add(8)?;
    let body = bytes.get(12..riff_end)?;
    let trailer = bytes.get(riff_end..)?;

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(b"RIFF\0\0\0\0WEBP");
    let mut pos = 0;
    while pos < body.len() {
        let fourcc: [u8; 4] = body.get(pos..pos + 4)?.try_into().ok()?;
        let size = usize::try_from(ByteOrder::Little.read_u32(body, pos + 4)?).ok()?;
        // Odd-sized chunks are followed by a pad byte.
        let end = pos
            .checked_add(8)?
            .checked_add(size)?
            .checked_add(size & 1)?;
        let chunk = body.get(pos..end)?;
        match &fourcc {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                if size < 10 {
                    return None;
                }
                let flags_at = out.len() + 8;
                out.extend_from_slice(chunk);
                *out.get_mut(flags_at)? &= !(VP8X_FLAG_EXIF | VP8X_FLAG_XMP);
            }
            _ => out.extend_from_slice(chunk),
        }
        pos = end;
    }
    let riff_size = u32::try_from(out.len() - 8).ok()?;
    out.get_mut(4..8)?.copy_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(trailer);
    Some(out)
}

// ---------------------------------------------------------------- sniffing

fn is_bmp(bytes: &[u8]) -> bool {
    // "BM" alone is too weak (plain text can start with it): also require a
    // known DIB header size.
    bytes.starts_with(b"BM")
        && ByteOrder::Little
            .read_u32(bytes, 14)
            .is_some_and(|dib| matches!(dib, 12 | 16 | 40 | 52 | 56 | 64 | 108 | 124))
}

fn sniff_iso_bmff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.get(4..8)? != b"ftyp" {
        return None;
    }
    let major: [u8; 4] = bytes.get(8..12)?.try_into().ok()?;
    let box_size = usize::try_from(ByteOrder::Big.read_u32(bytes, 0)?).ok()?;
    // Compatible brands follow the minor version; size 0/1 (to-EOF/64-bit) is
    // unusual for ftyp, so just scan what is present.
    let brands_end = match box_size {
        0 | 1 => bytes.len(),
        size => size.min(bytes.len()),
    };
    let (compatible, _) = bytes
        .get(16..brands_end)
        .unwrap_or_default()
        .as_chunks::<4>();

    let specific = |brand: &[u8; 4]| match brand {
        b"avif" | b"avis" => Some("avif"),
        b"heic" | b"heix" | b"hevc" | b"hevx" => Some("heic"),
        _ => None,
    };
    if let Some(format) = specific(&major) {
        return Some(format);
    }
    // Generic HEIF major brands (mif1/msf1) are shared by AVIF and HEIC, so
    // let a specific compatible brand decide first.
    let avif = compatible.iter().any(|b| specific(b) == Some("avif"));
    let heic = compatible.iter().any(|b| specific(b) == Some("heic"));
    let generic = std::iter::once(&major)
        .chain(compatible)
        .any(|b| b == b"mif1" || b == b"msf1");
    if avif {
        Some("avif")
    } else if heic || generic {
        Some("heic")
    } else {
        None
    }
}

const SVG_SNIFF_WINDOW: usize = 1024;

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = bytes
        .get(..bytes.len().min(SVG_SNIFF_WINDOW))
        .unwrap_or_default();
    let mut rest = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    loop {
        rest = rest.trim_ascii_start();
        let skipped = if let Some(tail) = rest.strip_prefix(b"<?") {
            after(tail, b"?>")
        } else if let Some(tail) = rest.strip_prefix(b"<!--") {
            after(tail, b"-->")
        } else if starts_with_ignore_case(rest, b"<!doctype") {
            skip_doctype(rest)
        } else {
            break;
        };
        match skipped {
            Some(tail) => rest = tail,
            None => return false,
        }
    }
    starts_with_ignore_case(rest, b"<svg")
        && rest
            .get(4)
            .is_some_and(|&b| b.is_ascii_whitespace() || b == b'>' || b == b'/')
}

fn starts_with_ignore_case(hay: &[u8], prefix: &[u8]) -> bool {
    hay.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// The bytes after the first occurrence of `needle`.
fn after<'a>(hay: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
    let at = hay.windows(needle.len()).position(|w| w == needle)?;
    hay.get(at + needle.len()..)
}

fn skip_doctype(rest: &[u8]) -> Option<&[u8]> {
    let close = rest.iter().position(|&b| b == b'>')?;
    match rest.iter().position(|&b| b == b'[') {
        // Internal subset: `<!DOCTYPE svg [ ... ]>`.
        Some(open) if open < close => {
            let tail = after(rest.get(open..)?, b"]")?;
            after(tail, b">")
        }
        _ => rest.get(close + 1..),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------ helpers

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn jpeg_segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, marker];
        out.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// Exif TIFF block with a fake camera, optional orientation and a GPS IFD
    /// at 0°N.
    fn exif_tiff(order: ByteOrder, orientation: Option<u16>) -> Vec<u8> {
        let entries = 2 + usize::from(orientation.is_some());
        let make_at = 8 + 2 + 12 * entries + 4;
        let gps_at = make_at + 8;
        let entry = |tag: u16, kind: u16, count: u32, value: [u8; 4]| {
            let mut e = Vec::new();
            e.extend_from_slice(&order.u16_bytes(tag));
            e.extend_from_slice(&order.u16_bytes(kind));
            e.extend_from_slice(&order.u32_bytes(count));
            e.extend_from_slice(&value);
            e
        };
        let mut tiff = Vec::new();
        tiff.extend_from_slice(order.tiff_marker());
        tiff.extend_from_slice(&order.u16_bytes(42));
        tiff.extend_from_slice(&order.u32_bytes(8));
        tiff.extend_from_slice(&order.u16_bytes(u16::try_from(entries).unwrap()));
        tiff.extend(entry(0x010F, 2, 8, order.u32_bytes(make_at as u32)));
        if let Some(value) = orientation {
            let [a, b] = order.u16_bytes(value);
            tiff.extend(entry(TIFF_TAG_ORIENTATION, 3, 1, [a, b, 0, 0]));
        }
        tiff.extend(entry(0x8825, 4, 1, order.u32_bytes(gps_at as u32)));
        tiff.extend_from_slice(&order.u32_bytes(0));
        assert_eq!(tiff.len(), make_at);
        tiff.extend_from_slice(b"TestCam\0");
        tiff.extend_from_slice(&order.u16_bytes(2));
        tiff.extend(entry(0x0000, 1, 4, [2, 3, 0, 0]));
        tiff.extend(entry(0x0001, 2, 2, [b'N', 0, 0, 0]));
        tiff.extend_from_slice(&order.u32_bytes(0));
        tiff
    }

    const SCAN: &[u8] = &[
        0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x12, 0xFF, 0x00, 0x34, 0xFF,
        0xD0, 0x56, 0xFF, 0xD9,
    ];

    /// A JPEG carrying every kind of metadata, and the offset of its SOS.
    fn rich_jpeg(order: ByteOrder, orientation: Option<u16>) -> (Vec<u8>, usize) {
        let mut exif = EXIF_HEADER.to_vec();
        exif.extend(exif_tiff(order, orientation));
        let mut out = vec![0xFF, 0xD8];
        out.extend(jpeg_segment(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0"));
        out.extend(jpeg_segment(0xE1, &exif));
        out.extend(jpeg_segment(
            0xE1,
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>TestCam</x:xmpmeta>",
        ));
        out.extend(jpeg_segment(
            0xE1,
            b"http://ns.adobe.com/xmp/extension/\0ABCDEF<rdf/>",
        ));
        out.extend(jpeg_segment(0xE2, b"ICC_PROFILE\0\x01\x01fake-icc"));
        out.extend(jpeg_segment(0xED, b"Photoshop 3.0\08BIM\x04\x04TestCam"));
        out.extend(jpeg_segment(0xFE, b"secret comment"));
        out.extend(jpeg_segment(0xEE, b"Adobe\0\x64\0\0\0\0\x01"));
        out.extend(jpeg_segment(0xDB, &[0u8; 65]));
        out.extend(jpeg_segment(0xC0, &[8, 0, 1, 0, 1, 1, 1, 0x11, 0]));
        out.extend(jpeg_segment(0xC4, &[0, 1, 0, 0, 0]));
        let sos = out.len();
        out.extend_from_slice(SCAN);
        (out, sos)
    }

    fn jpeg_markers(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut pos = 2;
        let mut markers = Vec::new();
        loop {
            let marker = bytes[pos + 1];
            if marker == 0xDA {
                markers.push((marker, Vec::new()));
                return markers;
            }
            let len = usize::from(u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]));
            markers.push((marker, bytes[pos + 4..pos + 2 + len].to_vec()));
            pos += 2 + len;
        }
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    }

    fn rich_png() -> Vec<u8> {
        let mut out = PNG_SIGNATURE.to_vec();
        out.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
        out.extend(png_chunk(b"iCCP", b"icc\0\0fake-compressed"));
        out.extend(png_chunk(b"gAMA", &45455u32.to_be_bytes()));
        out.extend(png_chunk(b"tEXt", b"Comment\0TestCam"));
        out.extend(png_chunk(
            b"iTXt",
            b"XML:com.adobe.xmp\0\0\0\0\0<x:xmpmeta>TestCam</x:xmpmeta>",
        ));
        out.extend(png_chunk(b"eXIf", &exif_tiff(ByteOrder::Big, Some(6))));
        out.extend(png_chunk(b"tIME", &[0x07, 0xE6, 1, 1, 0, 0, 0]));
        out.extend(png_chunk(b"pHYs", &[0, 0, 0x0B, 0x13, 0, 0, 0x0B, 0x13, 1]));
        out.extend(png_chunk(
            b"IDAT",
            &[0x78, 0x9C, 0x63, 0x60, 0x60, 0x60, 0, 0, 0, 4, 0, 1],
        ));
        out.extend(png_chunk(b"IEND", &[]));
        out
    }

    fn png_chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        let mut pos = 8;
        let mut chunks = Vec::new();
        while pos < bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = bytes[pos + 4..pos + 8].try_into().unwrap();
            chunks.push((kind, bytes[pos..pos + 12 + len].to_vec()));
            pos += 12 + len;
        }
        chunks
    }

    fn webp_chunk(fourcc: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = fourcc.to_vec();
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = chunks.concat();
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&u32::try_from(body.len() + 4).unwrap().to_le_bytes());
        out.extend_from_slice(b"WEBP");
        out.extend(body);
        out
    }

    const VP8X_ICC: u8 = 0x20;
    const VP8_DATA: &[u8] = b"fake-vp8-frame";
    const ICCP_DATA: &[u8] = b"fake-icc-odd";

    fn rich_webp() -> Vec<u8> {
        let flags = VP8X_ICC | VP8X_FLAG_EXIF | VP8X_FLAG_XMP;
        riff(&[
            webp_chunk(b"VP8X", &[flags, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            webp_chunk(b"ICCP", &ICCP_DATA[..11]),
            webp_chunk(b"VP8 ", &VP8_DATA[..13]),
            webp_chunk(b"EXIF", &exif_tiff(ByteOrder::Little, Some(6))),
            webp_chunk(b"XMP ", b"<x:xmpmeta>TestCam</x:xmpmeta>"),
        ])
    }

    fn webp_chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        let mut pos = 12;
        let mut chunks = Vec::new();
        while pos < bytes.len() {
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let end = pos + 8 + size + (size & 1);
            chunks.push((
                bytes[pos..pos + 4].try_into().unwrap(),
                bytes[pos..end].to_vec(),
            ));
            pos = end;
        }
        chunks
    }

    // ------------------------------------------------------------ JPEG

    #[test]
    fn jpeg_strips_exif_xmp_iptc_and_comments_but_keeps_colour_segments() {
        let (input, sos) = rich_jpeg(ByteOrder::Little, None);
        let out = strip_metadata(&input).expect("metadata removed");

        assert!(out.starts_with(&[0xFF, 0xD8]));
        assert!(out.ends_with(&input[sos..]), "scan data copied verbatim");
        let markers: Vec<u8> = jpeg_markers(&out).iter().map(|(m, _)| *m).collect();
        assert_eq!(markers, [0xE0, 0xE2, 0xEE, 0xDB, 0xC0, 0xC4, 0xDA]);
        assert!(contains(&out, b"JFIF\0"));
        assert!(contains(&out, b"ICC_PROFILE\0"));
        for leaked in [
            b"Exif".as_slice(),
            b"TestCam",
            b"http://ns.adobe.com",
            b"Photoshop",
            b"secret comment",
        ] {
            assert!(
                !contains(&out, leaked),
                "{:?} leaked",
                String::from_utf8_lossy(leaked)
            );
        }
    }

    #[test]
    fn jpeg_keeps_orientation_as_minimal_exif_in_both_byte_orders() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let (input, _) = rich_jpeg(order, Some(6));
            let out = strip_metadata(&input).expect("metadata removed");
            let segments = jpeg_markers(&out);
            let markers: Vec<u8> = segments.iter().map(|(m, _)| *m).collect();
            // Replacement sits where the first APP1 was (after APP0).
            assert_eq!(markers, [0xE0, 0xE1, 0xE2, 0xEE, 0xDB, 0xC0, 0xC4, 0xDA]);

            let payload = &segments[1].1;
            let tiff = payload.strip_prefix(EXIF_HEADER).expect("exif header");
            assert_eq!(&tiff[..2], order.tiff_marker());
            assert_eq!(order.read_u16(tiff, 8), Some(1), "single IFD0 entry");
            assert_eq!(exif_orientation(tiff), Some((order, 6)));
            assert_eq!(order.read_u32(tiff, 22), Some(0), "no next IFD");
            assert_eq!(tiff.len(), 26);
            assert!(!contains(&out, b"TestCam"));
        }
    }

    #[test]
    fn jpeg_orientation_one_drops_exif_entirely() {
        let (input, _) = rich_jpeg(ByteOrder::Big, Some(1));
        let out = strip_metadata(&input).expect("metadata removed");
        assert!(!jpeg_markers(&out).iter().any(|(m, _)| *m == 0xE1));
        assert!(!contains(&out, b"Exif"));
    }

    #[test]
    fn jpeg_with_corrupt_exif_drops_it_without_orientation() {
        let mut exif = EXIF_HEADER.to_vec();
        exif.extend_from_slice(b"II*\0\xFF\xFF\xFF\x7F"); // IFD0 offset out of range
        let mut input = vec![0xFF, 0xD8];
        input.extend(jpeg_segment(0xE1, &exif));
        input.extend(jpeg_segment(0xDB, &[0u8; 65]));
        input.extend_from_slice(SCAN);
        let out = strip_metadata(&input).expect("exif removed");
        assert!(!contains(&out, b"Exif"));
        assert_eq!(out.len(), input.len() - exif.len() - 4);
    }

    #[test]
    fn jpeg_misnamed_as_png_is_still_cleaned_as_jpeg() {
        let (input, _) = rich_jpeg(ByteOrder::Little, Some(3));
        assert_eq!(strip_metadata(&input), strip_metadata(&input));
        assert!(strip_metadata(&input).is_some());
    }

    #[test]
    fn jpeg_exif_parser_rejects_bad_orientation_entries() {
        let order = ByteOrder::Little;
        let tiff = exif_tiff(order, Some(9));
        assert_eq!(exif_orientation(&tiff), None);
        assert_eq!(exif_orientation(&exif_tiff(order, None)), None);
        assert_eq!(exif_orientation(b"II*\0"), None);
        assert_eq!(exif_orientation(b"XX*\0\x08\0\0\0"), None);
    }

    // ------------------------------------------------------------ PNG

    #[test]
    fn png_strips_text_exif_and_time_chunks_only() {
        let input = rich_png();
        let out = strip_metadata(&input).expect("metadata removed");

        assert!(out.starts_with(PNG_SIGNATURE));
        let chunks = png_chunks(&out);
        let kinds: Vec<&[u8; 4]> = chunks.iter().map(|(k, _)| k).collect();
        assert_eq!(
            kinds,
            [b"IHDR", b"iCCP", b"gAMA", b"pHYs", b"IDAT", b"IEND"]
        );
        let original = png_chunks(&input);
        for (kind, bytes) in &chunks {
            assert!(original.iter().any(|(k, b)| k == kind && b == bytes));
            let len = bytes.len();
            let crc = u32::from_be_bytes(bytes[len - 4..].try_into().unwrap());
            assert_eq!(crc, crc32(&bytes[4..len - 4]), "CRC of {kind:?}");
        }
        assert!(!contains(&out, b"TestCam"));
    }

    // ------------------------------------------------------------ WebP

    #[test]
    fn webp_strips_exif_and_xmp_chunks_and_clears_vp8x_flags() {
        let input = rich_webp();
        let out = strip_metadata(&input).expect("metadata removed");

        let riff_size = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff_size, out.len() - 8);
        let chunks = webp_chunks(&out);
        let fourccs: Vec<&[u8; 4]> = chunks.iter().map(|(f, _)| f).collect();
        assert_eq!(fourccs, [b"VP8X", b"ICCP", b"VP8 "]);
        assert_eq!(chunks[0].1[8], VP8X_ICC, "only the ICC flag remains");
        let original = webp_chunks(&input);
        assert_eq!(chunks[1], original[1], "odd ICCP chunk keeps its pad byte");
        assert_eq!(chunks[2], original[2]);
        assert!(!contains(&out, b"TestCam"));
    }

    // ------------------------------------------------------------ clean, broken, unsupported

    fn clean_inputs() -> Vec<(&'static str, Vec<u8>)> {
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.extend(jpeg_segment(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0"));
        jpeg.extend(jpeg_segment(0xE2, b"ICC_PROFILE\0\x01\x01fake-icc"));
        jpeg.extend(jpeg_segment(0xDB, &[0u8; 65]));
        jpeg.extend_from_slice(SCAN);

        let mut png = PNG_SIGNATURE.to_vec();
        png.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
        png.extend(png_chunk(b"IDAT", &[0x78, 0x9C, 0x63, 0x60, 0, 0, 0, 1]));
        png.extend(png_chunk(b"IEND", &[]));

        let simple_webp = riff(&[webp_chunk(b"VP8 ", VP8_DATA)]);
        let vp8x_webp = riff(&[
            webp_chunk(b"VP8X", &[VP8X_ICC, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            webp_chunk(b"ICCP", ICCP_DATA),
            webp_chunk(b"VP8 ", VP8_DATA),
        ]);
        vec![
            ("jpg", jpeg),
            ("png", png),
            ("webp", simple_webp),
            ("webp", vp8x_webp),
        ]
    }

    #[test]
    fn clean_inputs_return_none() {
        for (ext, input) in clean_inputs() {
            assert_eq!(strip_metadata(&input), None, "{ext}");
        }
    }

    #[test]
    fn stripping_is_idempotent() {
        let inputs = [
            ("jpg", rich_jpeg(ByteOrder::Big, Some(6)).0),
            ("jpg", rich_jpeg(ByteOrder::Little, None).0),
            ("png", rich_png()),
            ("webp", rich_webp()),
        ];
        for (ext, input) in inputs {
            let once = strip_metadata(&input).expect("first pass strips");
            assert_eq!(strip_metadata(&once), None, "{ext}");
        }
    }

    #[test]
    fn truncated_inputs_return_none_without_panicking() {
        let (jpeg, sos) = rich_jpeg(ByteOrder::Little, Some(6));
        for len in 0..jpeg.len() {
            let result = strip_metadata(&jpeg[..len]);
            // Once SOS is reached the (truncated) scan is copied verbatim.
            if len < sos + 2 {
                assert_eq!(result, None, "jpeg truncated to {len}");
            }
        }
        for (ext, input) in [("png", rich_png()), ("webp", rich_webp())] {
            for len in 0..input.len() {
                assert_eq!(
                    strip_metadata(&input[..len]),
                    None,
                    "{ext} truncated to {len}"
                );
            }
        }
    }

    #[test]
    fn corrupted_inputs_never_panic() {
        let fixtures = [
            ("jpg", rich_jpeg(ByteOrder::Big, Some(6)).0),
            ("png", rich_png()),
            ("webp", rich_webp()),
        ];
        let mut seed = 0x2545_F491_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for (_format, fixture) in &fixtures {
            for _ in 0..3000 {
                let mut bytes = fixture.clone();
                for _ in 0..(next() % 4 + 1) {
                    let at = next() as usize % bytes.len();
                    bytes[at] = next() as u8;
                }
                let _ = strip_metadata(&bytes);
            }
        }
        let prefixes: [&[u8]; 3] = [
            PNG_SIGNATURE,
            &[0xFF, 0xD8, 0xFF],
            b"RIFF\xFF\xFF\xFF\xFFWEBP",
        ];
        for prefix in prefixes {
            for _ in 0..2000 {
                let mut bytes = prefix.to_vec();
                bytes.extend((0..next() % 64).map(|_| next() as u8));
                let _ = strip_metadata(&bytes);
            }
        }
    }

    #[test]
    fn malformed_lengths_return_none() {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        png.extend_from_slice(b"tEXt");
        assert_eq!(strip_metadata(&png), None);

        let mut webp = rich_webp();
        webp[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(strip_metadata(&webp), None);

        let mut short_vp8x = riff(&[webp_chunk(b"VP8X", &[VP8X_FLAG_EXIF, 0, 0])]);
        assert_eq!(strip_metadata(&short_vp8x), None);
        short_vp8x.truncate(12);
        assert_eq!(strip_metadata(&short_vp8x), None);

        for len in [0u8, 1] {
            let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, len];
            jpeg.extend_from_slice(SCAN);
            assert_eq!(strip_metadata(&jpeg), None);
        }
    }

    #[test]
    fn unsupported_formats_are_left_alone() {
        let mut heic = 24u32.to_be_bytes().to_vec();
        heic.extend_from_slice(b"ftypheic\0\0\0\0mif1heic");
        let inputs: [(&str, &[u8]); 5] = [
            ("gif", b"GIF89a\x01\0\x01\0\0\0\0;"),
            ("tiff", b"II*\0\x08\0\0\0\0\0"),
            ("heic", &heic),
            (
                "svg",
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- TestCam --></svg>",
            ),
            ("png", b"just a text file named .png"),
        ];
        for (ext, bytes) in inputs {
            assert_eq!(strip_metadata(bytes), None, "{ext}");
        }
    }

    // ------------------------------------------------------------ real codecs

    #[test]
    fn stripped_real_images_still_decode() {
        use image::{ImageFormat, RgbImage};

        let pixels = RgbImage::from_fn(8, 8, |x, y| image::Rgb([x as u8 * 30, y as u8 * 30, 99]));
        for format in [ImageFormat::Jpeg, ImageFormat::Png] {
            let mut encoded = std::io::Cursor::new(Vec::new());
            pixels.write_to(&mut encoded, format).unwrap();
            let encoded = encoded.into_inner();

            // Inject metadata right after the signature / SOI.
            let (split, metadata) = match format {
                ImageFormat::Jpeg => {
                    let mut exif = EXIF_HEADER.to_vec();
                    exif.extend(exif_tiff(ByteOrder::Big, Some(6)));
                    (
                        2,
                        [jpeg_segment(0xE1, &exif), jpeg_segment(0xFE, b"TestCam")].concat(),
                    )
                }
                _ => {
                    let ihdr_end = PNG_SIGNATURE.len() + 12 + 13;
                    (ihdr_end, png_chunk(b"tEXt", b"Comment\0TestCam"))
                }
            };
            let mut dirty = encoded[..split].to_vec();
            dirty.extend(metadata);
            dirty.extend_from_slice(&encoded[split..]);

            let clean = strip_metadata(&dirty).expect("metadata removed");
            assert!(!contains(&clean, b"TestCam"));
            let original = image::load_from_memory_with_format(&encoded, format).unwrap();
            let decoded = image::load_from_memory_with_format(&clean, format).unwrap();
            assert_eq!(decoded.to_rgb8(), original.to_rgb8(), "{format:?}");
        }
    }

    // ------------------------------------------------------------ sniffing

    #[test]
    fn sniffs_every_supported_format() {
        let ftyp = |major: &[u8; 4], compatible: &[&[u8; 4]]| {
            let mut out = u32::try_from(16 + 4 * compatible.len())
                .unwrap()
                .to_be_bytes()
                .to_vec();
            out.extend_from_slice(b"ftyp");
            out.extend_from_slice(major);
            out.extend_from_slice(&[0, 0, 0, 0]);
            for brand in compatible {
                out.extend_from_slice(*brand);
            }
            out.extend_from_slice(b"\0\0\0\x08meta");
            out
        };
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0u8; 12]);
        bmp.extend_from_slice(&40u32.to_le_bytes());

        let cases: Vec<(Vec<u8>, &str)> = vec![
            (rich_png(), "png"),
            (rich_jpeg(ByteOrder::Little, None).0, "jpeg"),
            (b"GIF87a\x01\0".to_vec(), "gif"),
            (b"GIF89a\x01\0".to_vec(), "gif"),
            (rich_webp(), "webp"),
            (bmp, "bmp"),
            (vec![0, 0, 1, 0, 1, 0, 16, 16], "ico"),
            (b"II*\0\x08\0\0\0".to_vec(), "tiff"),
            (b"MM\0*\0\0\0\x08".to_vec(), "tiff"),
            (ftyp(b"avif", &[b"mif1", b"miaf"]), "avif"),
            (ftyp(b"avis", &[]), "avif"),
            (ftyp(b"mif1", &[b"mif1", b"avif"]), "avif"),
            (ftyp(b"heic", &[b"mif1", b"heic"]), "heic"),
            (ftyp(b"mif1", &[b"mif1", b"heix"]), "heic"),
            (ftyp(b"msf1", &[b"hevc"]), "heic"),
            (ftyp(b"mif1", &[]), "heic"),
            (b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(), "svg"),
            (b"\xEF\xBB\xBF \n\t<SVG>".to_vec(), "svg"),
            (
                b"<?xml version=\"1.0\"?>\n<!-- made by TestCam -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" [ <!ENTITY a \"b\"> ]>\n<svg\nwidth=\"1\"/>"
                    .to_vec(),
                "svg",
            ),
        ];
        for (bytes, expected) in cases {
            assert_eq!(sniff_image_format(&bytes), Some(expected), "{expected}");
        }
    }

    #[test]
    fn rejects_non_images() {
        let mut late_svg = vec![b' '; SVG_SNIFF_WINDOW];
        late_svg.extend_from_slice(b"<svg/>");
        let mut mp4 = 20u32.to_be_bytes().to_vec();
        mp4.extend_from_slice(b"ftypisom\0\0\0\0mp41");
        let cases: [&[u8]; 10] = [
            b"",
            b"hello, this text file is named image.png",
            b"BMW owners manual",
            b"<html><body><svg/></body></html>",
            b"<svgfoo/>",
            b"<?xml version=\"1.0\"",
            b"RIFF\0\0\0\0WAVEfmt ",
            &[0, 0, 1, 0, 0, 0],
            &mp4,
            &late_svg,
        ];
        for bytes in cases {
            assert_eq!(
                sniff_image_format(bytes),
                None,
                "{:?}",
                String::from_utf8_lossy(bytes)
            );
        }
    }
}
