use std::time::Duration;
use std::rc::Rc;
use std::net::TcpStream;
use std::thread;
use std::process;

use anyhow::Error;
use isahc::{prelude::*, Request};
use log::error;
use log::info;
use url::Url;
use tungstenite::{connect, WebSocket, stream::MaybeTlsStream};

use crate::models::{CodeFunction, Repository, Scan, CodeIssue, GenericModel, MassIssues};
use super::config::Config;

pub type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

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

/// Perform an authentication process with the scheduler
pub fn authenticate_runner(shared_config: Rc<Config>, websocket: &mut Ws) {

    websocket.write_message(tungstenite::Message::Text(shared_config.scheduler.token.to_string())).unwrap_or_else(|err| {
        error!("Could not send authentication request, check the network connection ({})", err);
        process::exit(1);
    });
    let auth_response = websocket.read_message().unwrap_or_else(|err| {
        error!("Could not receive authentication response, check the network connection ({})", err);
        process::exit(1);
    });

    match auth_response.to_text() {
        Ok("auth-ok") => {
            info!("Authentication done, the runner is ready to perform scans");
        }
        _ => {
            error!("Authentication failed, check the runner token");
            process::exit(1);
        }
    }
}

/// Try to perform a websocket connection with the scheduler
pub fn try_scheduler_ws_connection(shared_config: Rc<Config>, websocket_url: &Url) -> Ws {

    let mut some_websocket: Option<Ws> = None;

    let mut retry_period = shared_config.scheduler.retry_period;
    let retry_scale_factor = shared_config.scheduler.retry_scale_factor;
    let retry_scale_limit = shared_config.scheduler.retry_scale_limit;
    
    loop {

        let mut is_connected = false;

        match connect(websocket_url) {
            Ok((socket, _response)) => {

                some_websocket = Some(socket);
                is_connected = true;

            },
            Err(err) => {

                error!("Scheduler connection failed: {}", err);
                error!("Retry in {} seconds", retry_period);

                let duration = Duration::from_secs(retry_period);
                thread::sleep(duration);

                if retry_period < retry_scale_limit {
                    retry_period = ((retry_period as f32) * retry_scale_factor).round() as u64;
                }

            }
        };

        if is_connected && some_websocket.is_some() {
            return some_websocket.unwrap_or_else(|| {
                error!("Expected a valid websocket, internal logic error");
                process::exit(1);
            });
        }

    }
}
