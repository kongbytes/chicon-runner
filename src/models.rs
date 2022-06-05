use std::path::Path;
use std::rc::Rc;

use anyhow::{bail, Error};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum MetricValue {

    Number(i64),

    Text(String),

    Boolean(bool)

}

#[derive(Deserialize, Serialize)]
pub struct CodeIssue {

    pub name: String,

    #[serde(rename = "repositoryId")]
    pub repository_id: Option<String>,

}

#[derive(Serialize)]
pub struct Scan {

    #[serde(rename = "functionId")]
    pub function_id: String,

    #[serde(rename = "repositoryId")]
    pub repository_id: String,

    pub commit: GitCommit,

    #[serde(rename = "hasFailed")]
    pub has_failed: bool,

    pub logs: String,

    #[serde(rename = "timingMs")]
    pub timing_ms: usize,

    pub results: Vec<ScanMetadata>

}

#[derive(Serialize)]
pub struct ScanMetadata {

    pub key: String,

    pub description: String,

    pub value: MetricValue

}

#[derive(Deserialize)]
pub struct FunctionEnv {

    pub name: String,

    #[serde(rename = "baseImage")]
    pub base_image: String,
    
    #[serde(rename = "fileExtension")]
    pub file_extension: String,
    
    pub executor: String,

    pub user: Option<String>

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

    pub content: String,

    pub outputs: Vec<FunctionOutput>

}

#[derive(Deserialize)]
pub struct FunctionOutput {

    pub key: String,

    pub description: String,

    #[serde(rename = "dataType")]
    pub data_type: String, // TODO Should be enum

    #[serde(rename = "defaultValue")]
    pub default_value: MetricValue

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

#[derive(Serialize, Clone)]
pub struct GitCommit {

    #[serde(rename = "commitId")]
    pub commit_id: String,

    pub message: Option<String>,
    
    pub branch: String

}

impl<'repo> From<git2::Commit<'repo>> for GitCommit {

    fn from(commit: git2::Commit) -> GitCommit {

        GitCommit {
            commit_id: commit.id().to_string(),
            message: commit.message().map(|message| message.to_string()),
            branch: "master".to_string() // TODO
        }
    }

}

impl Repository {

    pub fn pull_or_clone(&self, config: Rc<Config>) -> Result<GitCommit, Error> {

        let default_branch = "master";  // TODO

        let repository_path = Path::new(&config.workspace.path).join(&self.public_id).join("repository");
        let git_path = repository_path.join(".git");

        if git_path.is_dir() {

            let existing = git2::Repository::open(repository_path)?;
            existing.find_remote("origin")?.fetch(&[default_branch], None, None)?;

            let fetch_head = existing.find_reference("FETCH_HEAD")?;
            let fetch_commit = existing.reference_to_annotated_commit(&fetch_head)?;
            let (merge_analysis, _) = existing.merge_analysis(&[&fetch_commit])?;

            if merge_analysis.is_up_to_date() {

                let commit = existing.find_commit(fetch_commit.id())?;
                return Ok(commit.into());
            } 
            if !merge_analysis.is_fast_forward() {
                bail!("Fast-forward only authorized");
            }

            // Perform a fast-forward merge (Git pull)
            let refname = format!("refs/heads/{}", default_branch);
            let mut reference = existing.find_reference(&refname)?;
            reference.set_target(fetch_commit.id(), "Fast-Forward")?;
            existing.set_head(&refname)?;
            existing.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;

            let commit = existing.find_commit(fetch_commit.id())?;
            return Ok(commit.into())
        }
        

        let cloned = git2::Repository::clone(&self.url, repository_path)?;

        let fetch_head = cloned.find_reference("HEAD")?;
        let fetch_commit = cloned.reference_to_annotated_commit(&fetch_head)?;
        let commit = cloned.find_commit(fetch_commit.id())?;
    
        Ok(commit.into())
    }

}
