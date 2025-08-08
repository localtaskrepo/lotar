use crate::api_server;
use include_dir::{Dir, include_dir};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

// Use the target/web folder to keep all build artifacts together
const STATIC_FILES: Dir = include_dir!("target/web");

pub fn serve(api_server: &api_server::ApiServer, port: u16) {
    add_files_to_executable();

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", port, e);
            return;
        }
    };
    println!("Listening on port {}", port);
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buffer = [0; 512];
                let _ = stream.read(&mut buffer).unwrap_or(0); // Handle the read amount properly
                let request = String::from_utf8_lossy(&buffer);
                let request_line = match request.lines().next() {
                    Some(line) => line,
                    None => {
                        continue;
                    }
                };
                let request_parts: Vec<&str> = request_line.split(" ").collect();
                let request_path = request_parts[1];

                // Check if the request path starts with "/api"
                if request_path.starts_with("/api") {
                    // Execute the appropriate rust code to handle the API request
                    let handler_response = api_server.handle_request(request_path);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        handler_response.len(),
                        handler_response
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                } else {
                    // Get the file path to serve based on the request path
                    let file_path = format!("target/web{}", request_path);
                    match fs::File::open(&file_path) {
                        Ok(mut file) => {
                            let mut file_content = String::new();
                            if let Err(e) = file.read_to_string(&mut file_content) {
                                eprintln!("Failed to read file {}: {}", file_path, e);
                                continue;
                            }

                            let path: &Path = Path::new(&file_path);
                            let extension = match path.extension() {
                                Some(ext) => ext,
                                None => OsStr::new(""),
                            };

                            let content_type = match extension.to_str() {
                                Some("html") => "text/html",
                                Some("jpg") => "image/jpeg",
                                Some("png") => "image/png",
                                Some("css") => "text/css",
                                Some("js") => "application/javascript",
                                _ => "application/octet-stream",
                            };

                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
                                content_type,
                                file_content.len(),
                                file_content
                            );
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();
                        }
                        Err(_) => {
                            let response = "HTTP/1.1 404 NOT FOUND\r\n\r\n404 - Page not found.";
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();
                        }
                    }
                }
            }
            Err(e) => {
                // Log the error; avoid panicking on transient failures
                eprintln!("Connection error: {}", e);
            }
        }
    }
}

fn add_files_to_executable() -> HashMap<String, &'static [u8]> {
    let mut file_map = HashMap::new();
    for file in STATIC_FILES.files() {
        let path = format!("{}{}", "target/web", file.path().display());
        let key = match path.strip_prefix("target/web") {
            Some(k) => k.to_owned(),
            None => continue,
        };
        let data = file.contents();
        file_map.insert(key, data);
    }
    file_map
}
