//! DEV-216: attachment handling behind the web UI — metadata stripping and
//! not-an-image warnings on upload, thumbnails (`?w=`), the info endpoint,
//! reordering, the `has` list filter, and download names.
use base64::Engine;
use lotar::api_server::{ApiServer, HttpRequest, HttpResponse};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
mod common;
use crate::common::env_mutex::EnvVarGuard;

struct Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn workspace() -> Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard_tasks,
        _guard_fast,
    }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn request(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: query
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<HashMap<_, _>>(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn json_of(resp: &HttpResponse) -> Value {
    serde_json::from_slice(&resp.body).unwrap()
}

fn header<'a>(resp: &'a HttpResponse, name: &str) -> Option<&'a str> {
    resp.headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn create(api: &ApiServer, title: &str) -> String {
    let resp = api.handle_request(&request(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": title, "project": "IMG"}),
    ));
    assert_eq!(resp.status, 201, "{}", String::from_utf8_lossy(&resp.body));
    json_of(&resp)["data"]["id"].as_str().unwrap().to_string()
}

fn upload(api: &ApiServer, id: &str, filename: &str, bytes: &[u8]) -> Value {
    let resp = api.handle_request(&request(
        "POST",
        "/api/tasks/attachments/upload",
        &[],
        json!({
            "id": id,
            "filename": filename,
            "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        }),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    json_of(&resp)["data"].clone()
}

fn hash_of(stored: &str) -> String {
    lotar::services::attachment_service::AttachmentService::extract_hash_tag(stored).unwrap()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

/// `png` plus a tEXt chunk inserted before IEND (fake camera, no real data).
fn png_with_text(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = png(width, height);
    let iend = bytes.len() - 12;
    let data = b"Software\0TestCam";
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(data);
    // CRC over type + data.
    let mut crc: u32 = 0xffff_ffff;
    for byte in b"tEXt".iter().chain(data.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    chunk.extend_from_slice(&(!crc).to_be_bytes());
    bytes.splice(iend..iend, chunk);
    bytes
}

#[test]
fn uploads_strip_metadata_and_warn_about_fake_images() {
    let fx = workspace();
    let api = server();
    let id = create(&api, "Photo");

    let data = upload(&api, &id, "photo.png", &png_with_text(8, 8));
    assert_eq!(data["metadata_removed"], json!(true));
    let stored = data["stored_path"].as_str().unwrap();
    let on_disk = std::fs::read(fx.tasks_dir.join("@attachments").join(stored)).unwrap();
    assert!(
        !on_disk
            .windows(8)
            .any(|w| w == b"TestCam\0" || w == b"Software")
    );
    assert_eq!(
        image::load_from_memory(&on_disk).unwrap().to_rgb8(),
        image::load_from_memory(&png(8, 8)).unwrap().to_rgb8()
    );

    let clean = upload(&api, &id, "clean.png", &png(9, 9));
    assert!(clean.get("metadata_removed").is_none());
    assert!(clean.get("warnings").is_none());

    let fake = upload(&api, &id, "fake.png", b"not an image");
    let warnings = fake["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].as_str().unwrap().contains("fake.png"));
}

#[test]
fn hash_route_serves_cached_thumbnails() {
    let _fx = workspace();
    let api = server();
    let id = create(&api, "Big");
    let stored = upload(&api, &id, "big.png", &png(1200, 600))["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/attachments/h/{}/big.png", hash_of(&stored));

    let resp = api.handle_request(&request("GET", &path, &[("w", "300")], json!({})));
    assert_eq!(resp.status, 200);
    assert_eq!(header(&resp, "content-type"), Some("image/jpeg"));
    let thumb = image::load_from_memory(&resp.body).unwrap();
    assert_eq!((thumb.width(), thumb.height()), (480, 240));
    assert_eq!(
        header(&resp, "etag").unwrap(),
        format!("\"{}-w480\"", hash_of(&stored))
    );

    // Already small enough: the original comes back.
    let small = upload(&api, &id, "small.png", &png(20, 20))["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = api.handle_request(&request(
        "GET",
        &format!("/api/attachments/h/{}/small.png", hash_of(&small)),
        &[("w", "96")],
        json!({}),
    ));
    assert_eq!(header(&resp, "content-type"), Some("image/png"));
    assert_eq!(image::load_from_memory(&resp.body).unwrap().width(), 20);

    let resp = api.handle_request(&request("GET", &path, &[("w", "wide")], json!({})));
    assert_eq!(resp.status, 400);
}

#[test]
fn info_reports_sizes_and_dimensions() {
    let _fx = workspace();
    let api = server();
    let id = create(&api, "Info");
    let image = upload(&api, &id, "shot.png", &png(30, 20))["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let text = upload(&api, &id, "notes.txt", b"hello")["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = api.handle_request(&request(
        "POST",
        "/api/attachments/info",
        &[],
        json!({"project": "IMG", "attachments": [image, text, "missing.0123456789abcdef0123456789abcdef.png"]}),
    ));
    assert_eq!(resp.status, 200);
    let items = json_of(&resp)["data"]["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2, "unknown attachments are left out");
    assert_eq!(items[0]["width"], json!(30));
    assert_eq!(items[0]["height"], json!(20));
    assert_eq!(items[1]["size"], json!(5));
    assert!(items[1].get("width").is_none());
}

#[test]
fn attachments_can_be_reordered() {
    let _fx = workspace();
    let api = server();
    let id = create(&api, "Order");
    let a = upload(&api, &id, "a.txt", b"a")["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    api.handle_request(&request(
        "POST",
        "/api/tasks/references/link/add",
        &[],
        json!({"id": id, "url": "https://example.com/spec"}),
    ));
    let b = upload(&api, &id, "b.png", &png(4, 4))["stored_path"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = api.handle_request(&request(
        "POST",
        "/api/tasks/attachments/reorder",
        &[],
        json!({"id": id, "attachments": [b, a]}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let task = json_of(&resp)["data"].clone();
    let attachments: Vec<&str> = task["references"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["attachment"].as_str())
        .collect();
    assert_eq!(attachments, vec![b.as_str(), a.as_str()]);
    let history = task["history"].as_array().unwrap();
    assert!(
        history
            .iter()
            .any(|entry| entry["changes"][0]["field"] == "attachments_reordered")
    );

    // Must be an exact permutation of the task's attachments.
    for bad in [json!([a]), json!([a, a]), json!([a, b, "other.txt"])] {
        let resp = api.handle_request(&request(
            "POST",
            "/api/tasks/attachments/reorder",
            &[],
            json!({"id": id, "attachments": bad}),
        ));
        assert_eq!(resp.status, 400, "{bad}");
    }
}

#[test]
fn list_filters_by_what_tasks_carry() {
    let _fx = workspace();
    let api = server();
    let with_image = create(&api, "With image");
    upload(&api, &with_image, "shot.png", &png(4, 4));
    let with_file = create(&api, "With file");
    upload(&api, &with_file, "notes.txt", b"notes");
    create(&api, "Plain");

    let ids = |has: &str| -> Vec<String> {
        let resp = api.handle_request(&request(
            "GET",
            "/api/tasks/list",
            &[("has", has)],
            json!({}),
        ));
        assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
        let body = json_of(&resp);
        let mut ids: Vec<String> = body["data"]["tasks"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|task| task["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    };
    let mut both = vec![with_image.clone(), with_file.clone()];
    both.sort();
    assert_eq!(ids("attachment"), both);
    assert_eq!(ids("image"), vec![with_image.clone()]);
    assert!(ids("link").is_empty());

    let resp = api.handle_request(&request(
        "GET",
        "/api/tasks/list",
        &[("has", "pdf")],
        json!({}),
    ));
    assert_eq!(resp.status, 400);
}

#[test]
fn download_names_keep_non_ascii_characters() {
    let _fx = workspace();
    let api = server();
    let id = create(&api, "Names");
    let stored = upload(&api, &id, "Größe ✓.txt", b"x")["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = api.handle_request(&request(
        "GET",
        &format!(
            "/api/attachments/h/{}/Gr%C3%B6%C3%9Fe%20%E2%9C%93.txt",
            hash_of(&stored)
        ),
        &[],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(
        header(&resp, "content-disposition"),
        Some("inline; filename=\"Gr__e _.txt\"; filename*=UTF-8''Gr%C3%B6%C3%9Fe%20%E2%9C%93.txt")
    );
}

#[test]
fn media_supports_byte_ranges() {
    let _fx = workspace();
    let api = server();
    let id = create(&api, "Clip");
    let stored = upload(&api, &id, "clip.mp4", b"0123456789")["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/attachments/h/{}/clip.mp4", hash_of(&stored));
    let ranged = |range: &str| {
        let mut req = request("GET", &path, &[], json!({}));
        req.headers.insert("range".into(), range.into());
        api.handle_request(&req)
    };

    let resp = ranged("bytes=2-5");
    assert_eq!(resp.status, 206);
    assert_eq!(resp.body, b"2345");
    assert_eq!(header(&resp, "content-range"), Some("bytes 2-5/10"));
    assert_eq!(header(&resp, "content-type"), Some("video/mp4"));

    assert_eq!(ranged("bytes=7-").body, b"789");
    assert_eq!(ranged("bytes=-3").body, b"789");
    assert_eq!(ranged("bytes=4-400").body, b"456789");
    let resp = ranged("bytes=20-30");
    assert_eq!(resp.status, 416);
    assert_eq!(header(&resp, "content-range"), Some("bytes */10"));
    // Multiple ranges fall back to the whole file.
    assert_eq!(ranged("bytes=0-1,4-5").status, 200);

    let resp = api.handle_request(&request("GET", &path, &[], json!({})));
    assert_eq!(resp.status, 200);
    assert_eq!(header(&resp, "accept-ranges"), Some("bytes"));
}
