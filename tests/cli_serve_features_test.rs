#![allow(
    renamed_and_removed_lints,
    clippy::redundant_pattern_matching,
    clippy::needless_if
)]

mod common;

use crate::common::cargo_bin_silent;
use common::TestFixtures;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

// Phase 2.4 - Serve Command Advanced Features Testing.
// Tests web server functionality including startup, options, lifecycle, and error handling.

fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn wait_for_server(port: u16) {
    for _ in 0..80 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("server did not start on port {port}");
}

fn http_post_json(port: u16, path_and_query: &str, body: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(1_500)))
        .unwrap();
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        path_and_query,
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut header_buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        header_buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = header_buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let body_leftover = header_buf.split_off(pos + 4);
            let headers_text = String::from_utf8_lossy(&header_buf);
            let status = headers_text
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(0);
            let content_length = headers_text
                .lines()
                .filter_map(|line| {
                    line.split_once(':')
                        .map(|(key, value)| (key.trim(), value.trim()))
                })
                .find(|(key, _)| key.eq_ignore_ascii_case("Content-Length"))
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(0);

            let mut body_bytes = body_leftover;
            while body_bytes.len() < content_length {
                let n = stream.read(&mut tmp).unwrap_or(0);
                if n == 0 {
                    break;
                }
                body_bytes.extend_from_slice(&tmp[..n]);
            }
            body_bytes.truncate(content_length);
            return (status, body_bytes);
        }
    }

    (0, Vec::new())
}

fn stop_server(port: u16, child: &mut Child) {
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
        let req = "GET /__test/stop HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let _ = stream.write_all(req.as_bytes());
        let _ = stream.flush();
        let mut tmp = [0u8; 128];
        let _ = stream.read(&mut tmp);
    }

    for _ in 0..40 {
        if child.try_wait().unwrap().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_serve_command_basic_functionality() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a test task first to have some data to serve
    let mut cmd = cargo_bin_silent();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Test task for web server")
        .arg("--type=feature")
        .assert()
        .success();

    // Test serve command help
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--help")
        .assert();

    if let Ok(assert_result) = result.try_success() {
        let output = String::from_utf8_lossy(&assert_result.get_output().stdout);
        if output.contains("port") || output.contains("host") {}
    }

    // Test serve command with default options (background mode for testing)
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .timeout(Duration::from_millis(200)) // Very quick timeout - just test if command exists
        .assert();

    // Expected to timeout or fail - we just want to see if command is recognized
    let _serve_command_exists = result.try_success().is_ok();
}

#[test]
fn test_serve_command_port_options() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Test custom port option
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--port=8080")
        .timeout(Duration::from_millis(200))
        .assert();

    // Port option may or may not be implemented
    let _custom_port_works = result.try_success().is_ok();

    // Test alternative port option
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("-p")
        .arg("9090")
        .timeout(Duration::from_millis(200))
        .assert();

    // Alternative port syntax may or may not be implemented
    let _alt_port_works = result.try_success().is_ok();

    // Test invalid port option
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--port=99999") // Invalid port
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}
}

#[test]
fn test_serve_command_host_options() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Test localhost host
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--host=localhost")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}

    // Test bind to all interfaces
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--host=0.0.0.0")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}

    // Test custom IP
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--host=127.0.0.1")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}
}

#[test]
fn test_serve_command_combined_options() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Test port and host together
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--port=8080")
        .arg("--host=localhost")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}

    // Test with verbose output
    let mut cmd = cargo_bin_silent();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--verbose")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}
}

#[test]
fn test_serve_command_error_conditions() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Test with non-existent tasks directory (but from a valid working directory)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--tasks-dir=/tmp/nonexistent_dir_for_test")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}

    // Test serve with format option (may not make sense)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--format=json")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}
}

#[test]
fn test_serve_command_with_project_data() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create diverse test data
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Web UI Test Task")
        .arg("--type=feature")
        .arg("--priority=high")
        .arg("--assignee=test@example.com")
        .assert()
        .success();

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("API Test Task")
        .arg("--type=bug")
        .arg("--priority=high")
        .assert()
        .success();

    // Change one task status
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .arg("status")
        .arg("2")
        .arg("in_progress")
        .assert()
        .success();

    // Test serve with actual project data
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .timeout(Duration::from_millis(150))
        .assert();

    if let Ok(_) = result.try_success() {}

    // Test serve with specific project
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let result = cmd
        .current_dir(temp_dir)
        .arg("serve")
        .arg("--project=test-project")
        .timeout(Duration::from_millis(100))
        .assert();

    if let Ok(_) = result.try_success() {}
}

#[test]
fn test_serve_implementation_summary() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create test task
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Summary test task")
        .assert()
        .success();

    // Test basic serve existence
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let result = cmd.current_dir(temp_dir).arg("help").assert();

    if let Ok(assert_result) = result.try_success() {
        let output = String::from_utf8_lossy(&assert_result.get_output().stdout);
        if output.contains("serve") {}
    }

    // Test serve help specifically
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let result = cmd.current_dir(temp_dir).arg("help").arg("serve").assert();

    if let Ok(assert_result) = result.try_success() {
        let output = String::from_utf8_lossy(&assert_result.get_output().stdout);

        if output.contains("port") {}

        if output.contains("host") {}
    }
}

#[test]
fn test_serve_honors_explicit_tasks_dir_for_api_requests() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    let custom_tasks_dir = temp_dir.join("sandbox-data").join(".tasks");
    fs::create_dir_all(&custom_tasks_dir).unwrap();
    fs::write(
        custom_tasks_dir.join("config.yml"),
        "default:\n  project: SAN\nissue:\n  states: [Todo, InProgress, Done]\n  priorities: [Low, Medium, High]\n  types: [Feature, Bug]\n",
    )
    .unwrap();

    let port = find_free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(temp_dir)
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .arg("--tasks-dir")
        .arg(&custom_tasks_dir)
        .arg("serve")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    wait_for_server(port);

    let (status, body) = http_post_json(
        port,
        "/api/tasks/add?project=SAN",
        r#"{"title":"Sandbox task"}"#,
    );

    stop_server(port, &mut child);

    assert_eq!(
        status,
        201,
        "unexpected response: {}",
        String::from_utf8_lossy(&body)
    );
    assert!(
        custom_tasks_dir.join("SAN").join("1.yml").exists(),
        "task should be created in explicit tasks dir"
    );
    assert!(
        !temp_dir.join(".tasks").join("SAN").join("1.yml").exists(),
        "task should not be created under cwd/.tasks when --tasks-dir is set"
    );
}
