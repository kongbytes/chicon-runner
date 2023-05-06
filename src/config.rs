use std::path::Path;
use std::{fs, env};

use anyhow::Error;
use serde::Deserialize;

pub const TOKEN_ENV: &'static str = "CHICON_TOKEN"; 

fn get_default_path() -> String {
    "chicon-workspace".to_string()
}

fn get_default_cache_limit() -> String {
    "200".to_string()
}

fn get_default_namespace() -> String {
    "kb".to_string()
}

fn get_default_base_url() -> String {
    "localhost:3000".to_string()
}

fn get_default_token() -> String {
    env::var(TOKEN_ENV).unwrap_or_default()
}

fn get_default_retry_period() -> u64 {
    5
}

fn get_default_retry_factor() -> f32 {
    1.25
}

fn get_default_retry_limit() -> u64 {
    30
}


#[derive(Deserialize)]
pub struct ConfigWorkspace {

    #[serde(default = "get_default_path")]
    pub path: String,

    #[serde(default = "get_default_path")]
    pub cache_limit: String,

    pub ssh_clone_key: Option<String>

}

impl Default for ConfigWorkspace {

    fn default() -> Self {

        ConfigWorkspace {
            path: get_default_path(),
            cache_limit: get_default_cache_limit(),
            ssh_clone_key: None
        }
    }

}

#[derive(Deserialize)]
pub struct ConfigScheduler {

    #[serde(default = "get_default_base_url")]
    pub base_url: String,

    #[serde(default = "get_default_token")]
    pub token: String,

    #[serde(default = "get_default_retry_period")]
    pub retry_period: u64,

    #[serde(default = "get_default_retry_factor")]
    pub retry_scale_factor: f32,

    #[serde(default = "get_default_retry_limit")]
    pub retry_scale_limit: u64

}

impl Default for ConfigScheduler {

    fn default() -> Self {

        ConfigScheduler {
            base_url: get_default_base_url(),
            token: get_default_token(),
            retry_period: get_default_retry_period(),
            retry_scale_factor: get_default_retry_factor(),
            retry_scale_limit: get_default_retry_limit(),
        }
    }

}

#[derive(Deserialize)]
pub struct ConfigContainer {

    #[serde(default = "get_default_namespace")]
    pub namespace: String,

}

impl Default for ConfigContainer {

    fn default() -> Self {

        ConfigContainer {
            namespace: get_default_namespace()
        }
    }

}


#[derive(Deserialize)]
pub struct Config {

    #[serde(default)]
    pub workspace: ConfigWorkspace,

    #[serde(default)]
    pub scheduler: ConfigScheduler,

    #[serde(default)]
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

impl Default for Config {

    fn default() -> Self {

        Config {
            workspace: ConfigWorkspace::default(),
            scheduler: ConfigScheduler::default(),
            container: ConfigContainer::default(),
        }
    }

}
