use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::io;

pub fn load_config_paths() -> io::Result<HashMap<String, String>> {
    let config_path = Path::new("input/config/paths.json");
    if !config_path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(config_path)?;
    let raw_paths: HashMap<String, String> = serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut expanded_paths = HashMap::new();
    let project_input_dir = std::env::current_dir()?.join("input");
    let input_dir_str = project_input_dir.to_string_lossy();

    for (id, path) in raw_paths {
        if path.is_empty() { continue; }
        let expanded = path.replace("$input", &input_dir_str);
        expanded_paths.insert(id, expanded);
    }

    Ok(expanded_paths)
}
