//! The library sees the embedded web UI only through the hook `main` sets
//! (DEV-497), so the tests that need real assets run the binary. They need
//! `npm run build:web` first, like every Rust build that embeds the UI.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

/// Kills the server when the test ends, also when it fails.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_sends_the_embedded_index_page() {
    let expected = lotar_web_assets::get("index.html.gz").expect("target/web-embed/index.html.gz is missing: run `npm run build:web` first");
    // An empty working directory, so the dev-mode fallback to target/web on
    // disk can't serve the page instead.
    let dir = tempfile::tempdir().expect("temp dir");
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_lotar"))
            .current_dir(dir.path())
            .args(["--tasks-dir", ".tasks", "serve", "--host", "127.0.0.1", "--port", "0"])
            .env_remove("LOTAR_TASKS_DIR")
            .env_remove("LOTAR_WEB_UI_PATH")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_TEST_SILENT", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn lotar serve"),
    );
    let stdout = BufReader::new(server.0.stdout.take().expect("piped stdout"));
    let address = stdout.lines().map_while(Result::ok).find_map(|line| line.strip_prefix("   URL: http://").map(str::to_string)).expect("serve printed no URL");

    let mut stream = TcpStream::connect(&address).expect("connect to lotar serve");
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nAccept-Encoding: gzip\r\n\r\n").expect("send request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    let split = response.windows(4).position(|window| window == b"\r\n\r\n").expect("response has a header");
    let head = String::from_utf8_lossy(&response[..split]);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("Content-Encoding: gzip"), "{head}");
    assert_eq!(&response[split + 4..], expected);
}
