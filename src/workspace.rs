use std::fs;
use std::fs::{read_to_string, OpenOptions};
use std::io::prelude::*;
use std::path::Path;

use anyhow::Error;

use crate::config::Config;

pub struct Workspace {

    base_path: String

}

impl Workspace {

    pub fn new(config: &Config) -> Workspace {

        Workspace {
            base_path: config.workspace.to_string()
        }
    }

    pub fn clean(&self, full_clean: bool) -> Result<(), Error> {

        if full_clean {
            fs::remove_dir_all(format!("{}/repository", self.base_path)).ok();
        }
        
        fs::remove_dir_all(format!("{}/bin", self.base_path)).ok();
        fs::remove_dir_all(format!("{}/result", self.base_path)).ok();
    
        fs::create_dir(format!("{}/bin", self.base_path))?;
        fs::create_dir(format!("{}/result", self.base_path))?;

        Ok(())
    }

    pub fn write_string(&self, relative_path: &str, content: &str) -> Result<(), Error> {

        let base_path = Path::new(&self.base_path);
        let absolute_path = base_path.join(relative_path);

        let mut workspace_file = OpenOptions::new()
                .read(false).write(true).create(true)
                .open(absolute_path)?;
        workspace_file.write_all(content.as_bytes())?;

        Ok(())
    }

    pub fn read_string(&self, relative_path: &str) -> Result<String, Error> {

        let base_path = Path::new(&self.base_path);
        let absolute_path = base_path.join(relative_path);

        let file_content = read_to_string(absolute_path)?;

        Ok(file_content)
    }

}
