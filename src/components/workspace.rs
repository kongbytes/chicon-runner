use std::fs;
use std::fs::{read_to_string, OpenOptions};
use std::io::prelude::*;
use std::rc::Rc;
use std::path::PathBuf;

use anyhow::{Error, bail};
use fs_extra::dir::get_size;
use log::warn;

use super::config::Config;

const DEFAULT_CACHE: u64 = 100_000_000;

pub struct Workspace {

    base_path: PathBuf,

    cache_size: u64

}

impl Workspace {

    pub fn new(config: Rc<Config>) -> Result<Workspace, Error> {

        let cache_size = config.get_cache_bytes().unwrap_or(DEFAULT_CACHE);

        let raw_path = config.workspace.path.to_string();
        let base_path = PathBuf::from(raw_path).canonicalize()?;

        let metadata = fs::metadata(&base_path)?;
        if !metadata.is_dir() {
            bail!("Expected a directory as workspace");
        }

        let current_size = get_size(&base_path)?;
        if current_size >= cache_size {
            bail!("Workspace is already full ({}/{})", current_size, cache_size);
        }

        let workspace = Workspace {
            base_path,
            cache_size
        };
        Ok(workspace)
    }

    pub fn get_path(&self) -> &str {
        self.base_path.to_str().unwrap_or_default()
    }

    pub fn clean(&self, repository_id: &str, full_clean: bool) -> Result<(), Error> {

        let base_repository = &self.base_path.join(repository_id);

        if full_clean {
            fs::remove_dir_all(base_repository).ok();
        }
        else {
            fs::remove_dir_all(base_repository.join("bin")).ok();
            fs::remove_dir_all(base_repository.join("result")).ok();
        }
    
        if !base_repository.is_dir() {
            fs::create_dir(base_repository)?;
        }
        
        fs::create_dir(base_repository.join("bin"))?;
        fs::create_dir(base_repository.join("result"))?;

        Ok(())
    }

    pub fn clean_bin(&self, repository_id: &str) -> Result<(), Error> {

        let base_repository = &self.base_path.join(repository_id);

        fs::remove_dir_all(base_repository.join("bin")).ok();
        fs::create_dir(base_repository.join("bin"))?;
        
        Ok(())
    }

    pub fn write_string(&self, repository_id: &str, relative_path: &str, content: &str) -> Result<(), Error> {

        let absolute_path = &self.base_path.join(repository_id).join(relative_path);

        let mut workspace_file: fs::File = OpenOptions::new()
                .read(false).create(true).append(true)
                .open(absolute_path)?;
        workspace_file.write_all(content.as_bytes())?;

        Ok(())
    }

    pub fn read_string(&self, repository_id: &str, relative_path: &str) -> Result<String, Error> {

        let absolute_path = &self.base_path.join(repository_id).join(relative_path);

        let file_content = read_to_string(absolute_path)?;

        Ok(file_content)
    }

    pub fn get_total_usage(&self) -> Result<u64, Error> {

        let workspace_size = get_size(&self.base_path)?;

        Ok(workspace_size)
    }

    pub fn prune_storage(&self) -> Result<(), Error> {

        for _ in 0..10 {

            let current_usage = self.get_total_usage()?;
            if current_usage < self.cache_size {
                return Ok(())
            }

            let storage_mb = current_usage / 1_000_000;
            warn!("Storage is over cache limit ({}Mb), selecting a path to delete", storage_mb);
    
            let paths = fs::read_dir(&self.base_path)?;
            let potential_dir = paths.into_iter()
                .find(|path| path.as_ref().unwrap().path().is_dir());

            match potential_dir {
                Some(trashed_dir) => {
                    fs::remove_dir_all(trashed_dir.unwrap().path())?;
                },
                None => {
                    warn!("Could not find a directory to delete in the workspace");
                }
            }
        }

        Ok(())
    }

}
