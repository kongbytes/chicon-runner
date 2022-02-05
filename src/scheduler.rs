use std::time::Duration;
use std::rc::Rc;

use anyhow::Error;
use isahc::{prelude::*, Request};

use crate::models::{CodeFunction, Repository, Scan};
use crate::config::Config;

pub struct Scheduler {

    base_url: String,

    config: Rc<Config>,

    default_duration: Duration

}

impl Scheduler {

    pub fn new(config: Rc<Config>) -> Scheduler {

        Scheduler {
            base_url: format!("http://{}/api/v1", config.scheduler.base_url),
            config,
            default_duration: Duration::from_secs(10)
        }
    }

    pub fn get_repository(&self, repository_id: &str) -> Result<Repository, Error> {

        let repository_url = format!("{}/repositories/{}", self.base_url, repository_id);
        let code_functions = Request::get(&repository_url)
            .header("Content-Type", "application/json")
            .header("Authorization", self.authorization_value())
            .timeout(self.default_duration)
            .body(())?
            .send()?
            .json::<Repository>()?;

        Ok(code_functions)
    }

    pub fn get_functions(&self) -> Result<Vec<CodeFunction>, Error> {

        let functions_url = format!("{}/functions", self.base_url);
        let code_functions = Request::get(&functions_url)
            .header("Content-Type", "application/json")
            .header("Authorization", self.authorization_value())
            .timeout(self.default_duration)
            .body(())?
            .send()?
            .json::<Vec<CodeFunction>>()?;

        Ok(code_functions)
    }

    pub fn store_scan(&self, scan: Scan) -> Result<(), Error> {

        let scan_url = format!("{}/scans", self.base_url);
        let request_body = serde_json::to_string(&scan)?;

        Request::post(&scan_url)    // TODO Check that failed HTTP status codes are processed
            .header("Content-Type", "application/json")
            .header("Authorization", self.authorization_value())
            .timeout(self.default_duration)
            .body(request_body)?
            .send()?;

        Ok(())
    }

    fn authorization_value(&self) -> String {
        format!("Bearer {}", self.config.scheduler.token)
    }

}

