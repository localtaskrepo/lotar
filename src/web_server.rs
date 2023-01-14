use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::collections::HashMap;
use include_dir::{include_dir, Dir};
use crate::api_server;

pub fn serve(api_server: &api_server::ApiServer) {
    add_files_to_executable();
    let listener = TcpListener::bind("127.0.0.1:8000").unwrap();
    println!("Listening on port 8000");
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buffer = [0; 512];
                stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer);
                let request_line = request.lines().next().unwrap();
                let request_parts: Vec<&str> = request_line.split(" ").collect();
                let request_path = request_parts[1];

                // Check if the request path starts with "/api"
                if request_path.starts_with("/api") {
                    // Execute the appropriate rust code to handle the API request
                    let handler_response = api_server.handle_request(request_path);
                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                           handler_response.len(),
                                           handler_response);
                    stream.write(response.as_bytes()).unwrap();
                    stream.flush().unwrap();
                } else {
                    // Get the file path to serve based on the request path
                    let file_path = format!("public{}", request_path);
                    match fs::File::open(&file_path) {
                        Ok(mut file) => {
                            let mut file_content = String::new();
                            file.read_to_string(&mut file_content).unwrap();
                            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                                                   file_content.len(),
                                                   file_content);
                            stream.write(response.as_bytes()).unwrap();
                            stream.write(response.as_bytes()).unwrap();
                            stream.flush().unwrap();
                        }
                        Err(_) => {
                            let response = "HTTP/1.1 404 NOT FOUND\r\n\r\n404 - Page not found.";
                            stream.write(response.as_bytes()).unwrap();
                            stream.flush().unwrap();
                        }
                    }
                }
            }
            Err(e) => {
                let response = "HTTP/1.1 503 Service Unavailable\r\n\r\nService Unavailable.";
                let mut stream = std::net::TcpStream::connect("127.0.0.1:8000").unwrap();
                stream.write(response.as_bytes()).unwrap();
                stream.flush().unwrap();
                println!("Error: {}", e);
            }
        }
    }
}

const PUBLIC_FILES: Dir = include_dir!("public");

fn add_files_to_executable() -> HashMap<String, &'static [u8]> {
    let mut file_map = HashMap::new();
    for file in PUBLIC_FILES.files() {
        let path = format!("{}{}", "public", file.path().display());
        let key = path.strip_prefix("public").unwrap().to_owned();
        let data = file.contents();
        file_map.insert(key, data);
    }
    file_map
}