use std::path::Path;
use std::fs;

use anyhow::Error;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ConfigWorkspace {

    pub path: String,

    pub cache_limit: String,

    pub ssh_clone_key: Option<String>

}

#[derive(Deserialize)]
pub struct ConfigScheduler {

    pub base_url: String,

    pub token: String,

    pub retry_period: u64

}

#[derive(Deserialize)]
pub struct ConfigContainer {

    pub namespace: String,

}


#[derive(Deserialize)]
pub struct Config {

    pub workspace: ConfigWorkspace,

    pub scheduler: ConfigScheduler,

    pub container: ConfigContainer

}

impl Config {

    pub fn parse(config_path: &str) -> Result<Config, Error> {

        let path = Path::new(config_path);
        let content = fs::read_to_string(path)?;

        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    pub fn get_cache_bytes(&self) -> Result<u64, Error> {

        let cache_mb: u64 = self.workspace.cache_limit.parse()?;
        Ok(cache_mb * 1_000_000)
    }

}
