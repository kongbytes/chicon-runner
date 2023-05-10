use std::process;
use std::time::Duration;
use std::rc::Rc;
use std::thread;

use anyhow::{bail, Context, Error, Result};
use env_logger::Env;
use log::{info, error, warn};
use url::Url;
use serde::Deserialize;

use crate::models::CodeIssue;
use crate::components::{
    scheduler::{authenticate_runner, Scheduler, try_scheduler_ws_connection, Ws},
    workspace::Workspace,
    config::{Config, TOKEN_ENV},
    container::run_container
};

#[derive(PartialEq, Debug)]
struct ScanRequest {
    _version: String,
    repositories: Vec<String>,
    functions: Vec<String>
}

#[derive(Deserialize)]
struct IssueContainer {
    issues: Vec<CodeIssue>
}

pub fn launch_runner(config_path: Option<&str>, workspace_option: Option<&String>, ns_option: Option<&String>) -> Result<(), Error> {

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let mut config = match config_path {
        Some(path) => {
    
            info!("Loading TOML runner configuration from {}", path);
            Config::parse(path).unwrap_or_else(|err| {
                error!("Could not read or parse config file {} ({})", path, err);
                process::exit(1);
            })
  
        },
        None => {
            warn!("Using default runner configuration - this is not recommended for production");
            Config::default()
        }
    };

    if let Some(workspace_path) = workspace_option {
        config.set_workspace_path(workspace_path);
    }

    if let Some(namespace) = ns_option {
        config.set_container_namespace(namespace);
    }

    if config.scheduler.token.is_empty() {
        warn!("Runner token is empty, provide environment {} with authentication token", TOKEN_ENV);
    }

    let shared_config = Rc::new(config);

    let workspace = Workspace::new(shared_config.clone()).map_err(|err| {
        error!("Failure on storage, could not create workspace in directory '{}'", shared_config.workspace.path);
        err
    })?;
    let shared_workspace: Rc<Workspace> = Rc::new(workspace);
    info!("Initialized workspace in '{}' path, performing storage check", shared_workspace.get_path());

    let storage_usage = shared_workspace.get_total_usage().map_err(|err| {
        error!("Failure on storage, could not compute size of '{}' path", shared_config.workspace.path);
        err
    })?;
    let storage_mb = storage_usage / 1_000_000;
    info!("Workspace usage is currently {}Mb ({}Mb threshold before cleaning)", storage_mb, shared_config.workspace.cache_limit);

    let scheduler = Scheduler::new(shared_config.clone());
    let shared_scheduler = Rc::new(scheduler);

    let websocker_raw_url = format!("ws://{}/ws/runners", shared_config.scheduler.base_url);
    info!("Attempting connection on control plane ({})", websocker_raw_url);
    let websocket_url = Url::parse(&websocker_raw_url)?;

    loop {

        let mut websocket = try_scheduler_ws_connection(shared_config.clone(), &websocket_url);       
        info!("Connected to the scheduler, sending authentication request");

        authenticate_runner(shared_config.clone(), &mut websocket);

        loop {
            process_message(&mut websocket, shared_config.clone(), shared_scheduler.clone(), shared_workspace.clone()).unwrap_or_else(|err| {
                error!("Could not process message, {}", err);
            });
        }

    }
}


fn process_message(websocket: &mut Ws, shared_config: Rc<Config>, scheduler: Rc<Scheduler>, workspace: Rc<Workspace>) -> Result<(), Error> {

    let socket_read_result = websocket.read_message();

    if let Err(read_error) = socket_read_result {

        error!("Failed to read messages from scheduler: {}", read_error);

        let duration = Duration::from_secs(shared_config.scheduler.retry_period);
        thread::sleep(duration);

        return Ok(());
    }
    let raw_message = socket_read_result.unwrap_or_else(|err| {
        error!("Expected a valid message, internal logic error ({})", err);
        process::exit(1);
    });

    let decode_result = decode_message(raw_message);
    if decode_result.is_err() {
        error!("Could not read text message from socket");
        return Ok(()); 
    }
    let request = decode_result.unwrap_or_else(|err| {
        error!("Expected a decoded message, internal logic error ({})", err);
        process::exit(1);
    });
    let repository_id = if let Some(repository_id) = request.repositories.get(0) {
        repository_id
    }
    else {
        error!("Could not find repository to scan");
        return Ok(());  
    };
    info!("Received request for repository ID {} (functions {})", repository_id, request.functions.join(","));

    let code_functions = scheduler.get_functions(&request.functions).context("Failure when retrieving functions")?;
    let repository = scheduler.get_repository(repository_id).context("Failure when retrieving repository")?;

    workspace.clean(&repository.public_id, false)?;

    info!("Starting functions on repository {} with ID {} ({:?}, {:?})", repository.name, repository.public_id, repository.branch, repository.directory);
    
    let last_commit = repository.pull_or_clone(shared_config.clone())?;

    for code_function in code_functions.iter() {

        info!("Executing function \"{}\" (ID {})", code_function.name, code_function.public_id);

        let finished_scan = run_container(shared_config.clone(), &workspace, &repository.public_id , code_function, last_commit.clone())?;
        let scan_id = scheduler.store_scan(finished_scan)?;
        process_issues(&workspace, &repository.public_id, &scheduler, &code_function.public_id, &scan_id)?;
    }

    workspace.clean(&repository.public_id, false)?;
    workspace.prune_storage()?;

    Ok(())
}

/// Decodes a message received by the control plane (websocket)
fn decode_message(raw_message: tungstenite::Message) -> Result<ScanRequest, Error> {

    let runner_command: &str = raw_message.to_text()?;

    let message_parts: Vec<&str> = runner_command.split(';').collect();
    if message_parts.len() != 3 {
        bail!("Scan message should have 3 components, {} found ({})", message_parts.len(), runner_command);
    }

    let version = message_parts.first()
        .map(|m| m.to_string())
        .unwrap_or_default();
    if version != "v1" {
        bail!("Expected 'v1' scan message");
    }

    let repository_raw = message_parts.get(1).map(|message| message.trim());
    let repository_message = match repository_raw {
        Some(m) if m.len() > 0 => m,
        _ => bail!("Expected a non-empty repository identifier or wildcard"),
    };

    let function_raw = message_parts.get(2).map(|message| message.trim());
    let function_message = match function_raw {
        Some(m) if m.len() > 0 => m,
        _ => bail!("Expected non-empty function identifiers or wildcard"),
    };
    let functions: Vec<String> = function_message.split(',')
        .map(|m| m.to_string())
        .collect();

    let scan_request = ScanRequest {
        _version: version,
        repositories: vec![
            repository_message.into()
        ],
        functions
    };
    Ok(scan_request)
}

fn process_issues(workspace: &Workspace, repository_id: &str, scheduler: &Scheduler, function_id: &str, scan_id: &str) -> Result<(), Error> {

    let potential_issues = workspace.read_string(repository_id, "result/issues.toml");

    if let Ok(vulnerabilities_content) = potential_issues {

        let mut issue_list: Vec<CodeIssue> = vec![];
        let parsed_toml: Result<IssueContainer, toml::de::Error> = toml::from_str(&vulnerabilities_content);

        match parsed_toml {
            Ok(toml_results) => {
                issue_list = toml_results.issues;
            },
            Err(err) => {
                error!("Could not parse issue TOML for repository {}", repository_id);
                dbg!(err);
            }
        };

        let formatted_issues = issue_list.into_iter()
            .map(|issue_item| {
                CodeIssue {
                    name: issue_item.name,
                    scan_id: Some(scan_id.to_string()),
                    severity: issue_item.severity,
                    repository_id: Some(repository_id.to_string()),
                    function_id: Some(function_id.to_string())
                }
            })
            .collect();
        scheduler.store_issue(formatted_issues)?;
    }
    else {
        info!("No issues found linked to repository ID {}", repository_id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use anyhow::Error;
    use tungstenite::Message;
    use super::{decode_message, ScanRequest};

    #[test]
    fn should_decode_basic_message() -> Result<(), Error> {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;4ed8e41b-d226-4b4c-a55c-e22099173730");
        let expected_request = ScanRequest {
            _version: "v1".to_string(),
            repositories: vec!["7b2c112a-f7e5-4106-bffe-4734eb4fe49a".into()],
            functions: vec!["4ed8e41b-d226-4b4c-a55c-e22099173730".into()],
        };

        let decoded_message = decode_message(message)?;
        assert_eq!(expected_request, decoded_message);

        Ok(())
    }

    #[test]
    fn should_decode_multi_function_message() -> Result<(), Error> {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;4ed8e41b-d226-4b4c-a55c-e22099173730,aebe69bd-5245-4dff-aa0b-d7cbb6a4efdf");
        let expected_request = ScanRequest {
            _version: "v1".to_string(),
            repositories: vec![
                "7b2c112a-f7e5-4106-bffe-4734eb4fe49a".into(),
            ],
            functions: vec![
                "4ed8e41b-d226-4b4c-a55c-e22099173730".into(),
                "aebe69bd-5245-4dff-aa0b-d7cbb6a4efdf".into()
            ],
        };

        let decoded_message = decode_message(message)?;
        assert_eq!(expected_request, decoded_message);

        Ok(())
    }

    #[test]
    fn should_decode_wildcard_function_message() -> Result<(), Error> {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;*");
        let expected_request = ScanRequest {
            _version: "v1".to_string(),
            repositories: vec![
                "7b2c112a-f7e5-4106-bffe-4734eb4fe49a".into(),
            ],
            functions: vec!["*".into()],
        };

        let decoded_message = decode_message(message)?;
        assert_eq!(expected_request, decoded_message);

        Ok(())
    }

    #[test]
    fn should_reject_empty_function() {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;");
     
        assert!(matches!(
            decode_message(message), Err(_)
        ));
    }

    #[test]
    fn should_reject_empty_function_with_spaces() {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;       ");
     
        assert!(matches!(
            decode_message(message), Err(_)
        ));
    }

    #[test]
    fn should_reject_binary_message() {

        assert!(matches!(
            decode_message(Message::Binary(vec![0,1,0,1])), Err(_)
        ));
    }

    #[test]
    fn should_reject_ping_message() {

        assert!(matches!(
            decode_message(Message::Ping(vec![0])), Err(_)
        ));
    }

    #[test]
    fn should_reject_pong_message() {

        assert!(matches!(
            decode_message(Message::Ping(vec![1])), Err(_)
        ));
    }

    #[test]
    fn should_reject_close_message() {

        assert!(matches!(
            decode_message(Message::Close(None)), Err(_)
        ));
    }

    #[test]
    fn should_reject_too_many_components() {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;extra");

        assert!(matches!(
            decode_message(message), Err(_)
        ));
    }

    #[test]
    fn should_reject_without_functions() {

        let message = Message::text("v1;7b2c112a-f7e5-4106-bffe-4734eb4fe49a");

        assert!(matches!(
            decode_message(message), Err(_)
        ));
    }

    #[test]
    fn should_reject_without_version() {

        let message = Message::text("7b2c112a-f7e5-4106-bffe-4734eb4fe49a;4ed8e41b-d226-4b4c-a55c-e22099173730");

        assert!(matches!(
            decode_message(message), Err(_)
        ));
    }

    #[test]
    fn should_reject_unknown_versions() {

        let message = Message::text("v2;7b2c112a-f7e5-4106-bffe-4734eb4fe49a;4ed8e41b-d226-4b4c-a55c-e22099173730");

        assert!(matches!(
            decode_message(message), Err(_)
        ));
    }

}
