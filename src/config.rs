use std::path::Path;
use std::fs;

use anyhow::Error;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {

    pub workspace: String,

    pub scheduler: String,

    pub cache_size: String

}

impl Config {

    pub fn parse(config_path: &str) -> Result<Config, Error> {

        let path = Path::new(config_path);
        let content = fs::read_to_string(path)?;

        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    pub fn get_cache_bytes(&self) -> Result<u64, Error> {

        let cache_mb: u64 = self.cache_size.parse()?;
        Ok(cache_mb * 1_000_000)
    }

}
