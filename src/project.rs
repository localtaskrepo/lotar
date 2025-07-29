use std::fs;
use std::path::Path;
use regex::Regex;
use std::path::PathBuf;
use log::debug;

pub fn get_project_name() -> Option<String> {
    let current_dir = std::env::current_dir().unwrap();
    debug!("The current directory is: {}", current_dir.display());
    let mut project_name = None;
    let mut current_path = current_dir.clone();
    let mut found = false;

    let mut root = PathBuf::new();
    root.push("/");
    while project_name.is_none() && current_path != root {
        let next_path = current_path.clone();
        debug!("Checking path: {}", next_path.display());
        for entry in fs::read_dir(next_path).unwrap() {
            debug!("Checking file: {:?}", entry);
            let entry = entry.unwrap();
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if is_metadata_file(file_name) {
                project_name = extract_project_name(&path);
                found = true;
                break;
            }
        }
        if found {
            break;
        }
        current_path.pop();
    }
    project_name
}

fn is_metadata_file(file_name: &str) -> bool {
    // list of known metadata file extensions
    let metadata_file_extensions = vec![
        ".toml", ".yml", ".json", ".gradle", ".pom", ".sbt", ".scala", ".xml", ".pro", ".properties", ".dependencies", ".cargo"
    ];
    let result = metadata_file_extensions.iter().any(|extension| file_name.ends_with(extension));
    if result {
        debug!("{} is a metadata file", file_name);
    } else {
        debug!("{} is not a metadata file", file_name);
    }
    result
}

fn extract_project_name(file_path: &Path) -> Option<String> {
    let extension = file_path.extension()?.to_str()?;
    let file_content = fs::read_to_string(file_path).unwrap();

    match extension {
        "toml" => extract_project_name_from_toml(&*file_content),
        "yml" => extract_project_name_from_yml(&*file_content),
        "json" => extract_project_name_from_json(&*file_content),
        "gradle" => extract_project_name_from_gradle(&*file_content),
        "pom" => extract_project_name_from_pom(&*file_content),
        "sbt" => extract_project_name_from_sbt(&*file_content),
        "scala" => extract_project_name_from_scala(&*file_content),
        "xml" => extract_project_name_from_xml(&*file_content),
        "pro" => extract_project_name_from_pro(&*file_content),
        "properties" => extract_project_name_from_properties(&*file_content),
        "dependencies" => extract_project_name_from_dependencies(&*file_content),
        "cargo" => extract_project_name_from_cargo(&*file_content),
        _ => None,
    }
}

fn extract_project_name_from_toml(file_content: &str) -> Option<String> {
    debug!("Extracting project name from TOML file");
    let re = Regex::new(r#"name\s*=\s*"([^"]+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_json(file_content: &str) -> Option<String> {
    debug!("Extracting project name from JSON file");
    let re = Regex::new(r#""name"\s*:\s*"([^"]+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_xml(file_content: &str) -> Option<String> {
    debug!("Extracting project name from XML file");
    let re = Regex::new(r#"<name>([^<]+)</name>"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_yml(file_content: &str) -> Option<String> {
    debug!("Extracting project name from YAML file");
    let re = Regex::new(r#"name:\s*(\S+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_gradle(file_content: &str) -> Option<String> {
    debug!("Extracting project name from Gradle file");
    let re = Regex::new(r#"name\s*=\s*['"]([^'"]+)['"]"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_pom(file_content: &str) -> Option<String> {
    debug!("Extracting project name from POM file");
    let re = Regex::new(r#"<name>([^<]+)</name>"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_sbt(file_content: &str) -> Option<String> {
    debug!("Extracting project name from SBT file");
    let re = Regex::new(r#"name :=\s*["']([^"']+)["']"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_scala(file_content: &str) -> Option<String> {
    debug!("Extracting project name from Scala file");
    let re = Regex::new(r#"name :=\s*["']([^"']+)["']"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_pro(file_content: &str) -> Option<String> {
    debug!("Extracting project name from .pro file");
    let re = Regex::new(r#"PROJECT_NAME\s*=\s*(\S+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_properties(file_content: &str) -> Option<String> {
    debug!("Extracting project name from .properties file");
    let re = Regex::new(r#"project\.name\s*=\s*(\S+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_dependencies(file_content: &str) -> Option<String> {
    debug!("Extracting project name from .dependencies file");
    let re = Regex::new(r#"project_name\s*=\s*(\S+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

fn extract_project_name_from_cargo(file_content: &str) -> Option<String> {
    debug!("Extracting project name from Cargo.toml file");
    let re = Regex::new(r#"name\s*=\s*"([^"]+)"#).unwrap();
    match re.captures(&file_content) {
        Some(captures) => Some(captures[1].to_string()),
        None => None,
    }
}

pub fn get_project_path() -> Option<PathBuf> {
    Some(std::env::current_dir().unwrap())
}