use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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

    pub fn _has_tag(&self, search_tag: &str) -> bool {

        let result = self.tags.iter()
            .find(|repo_tag| *repo_tag == search_tag);

        return result.is_some();
    }

    pub fn clone(&self) {

        println!("Cloning {}", self.name);
        match git2::Repository::clone(&self.url, "workspace/repository") {
            Ok(repo) => {
    
                let branches = repo.branches(None).unwrap();
                for _branch in branches {
                    //println!("{:?}", branch.unwrap().0.name().unwrap());
                }
    
            },
            Err(e) => panic!("failed to clone: {}", e),
        };
    }

}
