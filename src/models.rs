use std::collections::HashMap;
use std::path::Path;

use anyhow::Error;
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Deserialize, Serialize)]
pub struct Scan {

    #[serde(rename = "functionId")]
    pub function_id: String,

    #[serde(rename = "repositoryId")]
    pub repository_id: String,

    #[serde(rename = "hasFailed")]
    pub has_failed: bool,

    pub logs: String,

    #[serde(rename = "timingMs")]
    pub timing_ms: usize,

    pub results: HashMap<String, String>

}

#[derive(Deserialize)]
pub struct FunctionEnv {

    pub name: String,

    #[serde(rename = "baseImage")]
    pub base_image: String,
    
    #[serde(rename = "fileExtension")]
    pub file_extension: String,
    
    pub executor: String,

}

#[derive(Deserialize)]
pub struct FunctionCapabilities {

    pub network: bool,

    pub filesystem: bool

}

#[derive(Deserialize)]
pub struct CodeFunction {

    #[serde(rename = "publicId")]
    pub public_id: String,

    pub name: String,

    pub environment: FunctionEnv,

    pub capabilities: FunctionCapabilities,

    pub content: String

}

#[derive(Deserialize)]
pub struct Repository {

    #[serde(rename = "publicId")]
    pub public_id: String,

    pub name: String,
    
    pub url: String,    // TODO Type, credentials, ...

    pub branch: Option<String>,
    
    pub directory: Option<String>,  // For monorepo use-cases 

    pub tags: Vec<String>

}

impl Repository {

    pub fn clone(&self, config: &Config) -> Result<(), Error> {

        let repo_path = Path::new(&config.workspace).join("repository");
        let _cloned_repo = git2::Repository::clone(&self.url, repo_path)?;
    
        /*let branches = cloned_repo.branches(None)?;
        for _branch in branches {
            println!("{:?}", branch.?.0.name()?);
        }*/

        Ok(())
    }

}
