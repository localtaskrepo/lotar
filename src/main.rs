mod project_name;
mod web_server;
mod config;
mod api_server;
mod routes;

fn main() {
    match project_name::get_project_name() {
        Some(project_name) => println!("Current project name: {}", project_name),
        None => println!("Could not find project name"),
    }

    // Example code to register handlers for different paths
    let mut api_server = api_server::ApiServer::new();

    routes::initialize(&mut api_server);

    web_server::serve(&api_server);
}
