use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Error};
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

    pub fn pull_or_clone(&self, config: &Config) -> Result<(), Error> {

        let repository_path = Path::new(&config.workspace).join(&self.public_id).join("repository");
        let git_path = repository_path.join(".git");

        if git_path.is_dir() {

            let existing = git2::Repository::open(repository_path)?;
            existing.find_remote("origin")?.fetch(&["master"], None, None)?;  // TODO Branch name

            let fetch_head = existing.find_reference("FETCH_HEAD")?;
            let fetch_commit = existing.reference_to_annotated_commit(&fetch_head)?;
            let (merge_analysis, _) = existing.merge_analysis(&[&fetch_commit])?;

            if merge_analysis.is_up_to_date() {
                return Ok(());
            } 
            if !merge_analysis.is_fast_forward() {
                bail!("Fast-forward only authorized");
            }

            // Perform a fast-forward merge (Git pull)
            let refname = format!("refs/heads/{}", "master");    // TODO Branch name
            let mut reference = existing.find_reference(&refname)?;
            reference.set_target(fetch_commit.id(), "Fast-Forward")?;
            existing.set_head(&refname)?;
            existing.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;

            return Ok(())
        }
        

        git2::Repository::clone(&self.url, repository_path)?;
    
        /*let branches = cloned_repo.branches(None)?;
        for _branch in branches {
            println!("{:?}", branch.?.0.name()?);
        }*/

        Ok(())
    }

}
