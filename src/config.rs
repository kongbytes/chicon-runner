use std::path::Path;
use std::fs;

use anyhow::Error;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {

    pub workspace: String,

    pub scheduler: String,

}

impl Config {

    pub fn parse(config_path: &str) -> Result<Config, Error> {

        let path = Path::new(config_path);
        let content = fs::read_to_string(path)?;

        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

}
