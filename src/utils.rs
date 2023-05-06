use std::{time::SystemTime, path::Path, process};

use anyhow::Error;

const GLOBAL_CONFIG_PATH: &str = "/etc/chicon/runner.toml";
const LOCAL_CONFIG: &str = "./runner.toml";

// Compute a time diff with milliseconds
pub fn compute_time_diff(start_time: SystemTime) -> Result<usize, Error> {

    let computed_diff = SystemTime::now()
        .duration_since(start_time)?
        .as_millis()
        .try_into()?;

    Ok(computed_diff)
}

/// Find a default config path
pub fn find_default_config(requested_path: Option<&String>) -> Option<&str> {

    if let Some(path) = requested_path {

        let existing_path = Path::new(path);

        if !existing_path.exists() {
            eprintln!("Runner configuration path {} not reachable", path);
            eprintln!("Ensure that the path is reachable and has the correct permissions");
            process::exit(1);
        }

        return Some(path);
    }

    let global_config = Path::new(GLOBAL_CONFIG_PATH);
    if global_config.exists() {
        return Some(GLOBAL_CONFIG_PATH);
    }

    let local_config = Path::new(LOCAL_CONFIG);
    if local_config.exists() {
        return Some(LOCAL_CONFIG);
    }
  
    None
    
}
