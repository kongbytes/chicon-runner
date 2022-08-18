use std::time::Duration;
use std::rc::Rc;

use anyhow::Error;
use isahc::{prelude::*, Request};

use crate::models::{CodeFunction, Repository, Scan, CodeIssue, GenericModel, MassIssues};
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

    pub fn get_functions(&self, functions: &[String]) -> Result<Vec<CodeFunction>, Error> {

        let functions_url = format!("{}/functions", self.base_url);
        let code_functions = Request::get(&functions_url)
            .header("Content-Type", "application/json")
            .header("Authorization", self.authorization_value())
            .timeout(self.default_duration)
            .body(())?
            .send()?
            .json::<Vec<CodeFunction>>()?;

        if functions.is_empty() {
            return Ok(code_functions)
        }

        if let Some("*") = functions.get(0).map(|first| first.as_ref()) {
            return Ok(code_functions)
        }

        let filtered_functions = code_functions.into_iter()
            .filter(|function| functions.contains(&function.public_id))
            .collect();
        Ok(filtered_functions)
    }

    pub fn store_scan(&self, scan: Scan) -> Result<String, Error> {

        let scan_url = format!("{}/scans", self.base_url);
        let request_body = serde_json::to_string(&scan)?;

        let scan_response = Request::post(&scan_url)    // TODO Check that failed HTTP status codes are processed
            .header("Content-Type", "application/json")
            .header("Authorization", self.authorization_value())
            .timeout(self.default_duration)
            .body(request_body)?
            .send()?
            .json::<GenericModel>()?;

        // TODO
        Ok(scan_response.public_id.unwrap_or_else(|| "-".to_string()))
    }

    pub fn store_issue(&self, issues: Vec<CodeIssue>) -> Result<(), Error> {

        let mass_issues = MassIssues {
            issues
        };

        let report_url = format!("{}/issues", self.base_url);
        let request_body = serde_json::to_string(&mass_issues)?;

        Request::post(&report_url)    // TODO Check that failed HTTP status codes are processed
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

