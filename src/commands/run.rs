use std::collections::HashMap;
use std::process::{self, Command};
use std::time::{SystemTime, Duration};
use std::rc::Rc;
use std::net::TcpStream;
use std::thread;

use anyhow::{bail, Context, Error, Result};
use log::{info, error, warn};
use tungstenite::{connect, WebSocket, stream::MaybeTlsStream};
use url::Url;
use serde::Deserialize;

use crate::config::{Config, TOKEN_ENV};
use crate::models::{CodeFunction, Scan, ScanMetadata, CodeIssue};
use crate::scheduler::Scheduler;
use crate::workspace::Workspace;

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

/// Try to perform a websocket connection with the scheduler
fn try_scheduler_ws_connection(shared_config: Rc<Config>, websocket_url: &Url) -> Ws {

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

/// Perform an authentication process with the scheduler
fn authenticate_runner(shared_config: Rc<Config>, websocket: &mut Ws) {

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

pub fn launch_runner(config_path: Option<&str>) -> Result<(), Error> {

    env_logger::init();

    let config = match config_path {
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

    if config.scheduler.token.is_empty() {
        warn!("Runner token is empty, provide environment {} with authentication token", TOKEN_ENV);
    }

    let shared_config = Rc::new(config);

    let workspace = Workspace::new(shared_config.clone());
    let shared_workspace = Rc::new(workspace);
    info!("Initialized workspace in '{}' path, performing storage check", shared_config.workspace.path);

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

struct ScanRequest {
    _version: String,
    repositories: Vec<String>,
    functions: Vec<String>
}

fn decode_message(raw_message: tungstenite::Message) -> Result<ScanRequest, Error> {

    if let Err(text_error) = raw_message.to_text() {
        bail!(text_error);
    }
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

    let repositories: Vec<String> = message_parts.get(1).unwrap_or(&"")
        .split(',')
        .map(|m| m.to_string())
        .collect();
    let functions: Vec<String> = message_parts.get(2).unwrap_or(&"")
        .split(',')
        .map(|m| m.to_string())
        .collect();

    let scan_request = ScanRequest {
        _version: version,
        repositories,
        functions
    };
    Ok(scan_request)
}

fn run_container(config: Rc<Config>, workspace: &Workspace, repository_id: &str, code_function: &CodeFunction, commit: crate::models::GitCommit) -> Result<Scan, Error> {

    workspace.clean(repository_id, false).context("Could not clean workspace before run")?;

    let mut timing_ms: usize = 0;
    let mut logs = "".to_string();
    let mut has_failed = false;

    let stage_total = code_function.stages.len();
    let mut stage_count = 0;

    for stage in &code_function.stages {

        stage_count += 1;
        info!("Executing stage of {}/{} \"{}\" : environment {} ({})", stage_count, stage_total, code_function.name, stage.environment.name, stage.environment.base_image);

        let script_path = format!("bin/process.{}", &stage.environment.file_extension);
        workspace.write_string(repository_id, &script_path, &stage.content)?;

        let namespace_arg = format!("--namespace={}", config.container.namespace);
        pull_image(&stage.environment.base_image, &namespace_arg).context("Could not pull container image")?;

        let mut nerdctl = Command::new("nerdctl");
        nerdctl
            .arg(&namespace_arg)
            .arg("run")
            .arg("--rm")
            // Security flags
            .arg("--cap-drop")
            .arg("all")
            .arg("--security-opt")
            .arg("apparmor=docker-default")
            .arg("--security-opt")
            .arg("no-new-privileges");

        if code_function.capabilities.network {
            nerdctl.arg("--network").arg("bridge");
        }
        else {
            nerdctl.arg("--network").arg("none");
        }
        
        if let Some(user) = &stage.environment.user {
            nerdctl.arg("--user").arg(user);
        }

        nerdctl.arg("--volume") // Volume mounting
            .arg(format!("{}/{}/repository:/workspace:ro", config.workspace.path, repository_id))
            .arg("--volume")
            .arg(format!("{}/{}/bin:/tmp-bin:ro", config.workspace.path, repository_id))
            .arg("--volume")
            .arg(format!("{}/{}/result:/result", config.workspace.path, repository_id))
            .arg("--workdir")
            .arg("/workspace");

        if !code_function.capabilities.filesystem {
            nerdctl.arg("--read-only");
        }
        
        // Binary
        nerdctl.arg(&stage.environment.base_image)
            .arg(&stage.environment.executor)
            .arg(format!("/tmp-bin/process.{}", &stage.environment.file_extension));
        
        let start_time = SystemTime::now();
        let output = nerdctl.output()?;
        timing_ms += crate::utils::compute_time_diff(start_time)?;

        let stderr_logs = String::from_utf8(output.stderr).unwrap_or_else(|_| "(invalid UTF8 string)".to_string());
        let stdout_logs = String::from_utf8(output.stdout).unwrap_or_else(|_| "(invalid UTF8 string)".to_string());
        logs.push_str(
            &format!("{}\n{}", stdout_logs, stderr_logs) // TODO More accurate mix
        );

        if !output.status.success() {
            has_failed = true;
        }

        workspace.clean_bin(repository_id)?;
    }

    let mut metric_results: Option<HashMap<String, crate::models::MetricValue>> = None;
    let potential_toml = workspace.read_string(repository_id, "result/data.toml");

    if let Ok(toml_content) = potential_toml {

        if let Ok(toml_results) = toml::from_str(&toml_content) {    // TODO TOML error handling
            metric_results = Some(toml_results);
        }
    }

    let results: Vec<ScanMetadata> = metric_results.unwrap_or_default().into_iter().map(|(result_key, result_value)| {
        
        let potential_description = code_function.outputs.iter()
            .find(|output| output.key == result_key)
            .map(|output| output.description.clone());

        ScanMetadata {
            key: result_key,
            description: potential_description.unwrap_or_default(),
            value: result_value
        }

    }).collect();

    let finished_scan = Scan {
        function_id: code_function.public_id.to_string(),
        repository_id: repository_id.to_string(),
        commit,
        has_failed,
        logs,
        timing_ms,
        results
    };
    Ok(finished_scan)
}

#[derive(Deserialize)]
struct IssueContainer {
    issues: Vec<CodeIssue>
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

fn pull_image(image_tag: &str, namespace_arg: &str) -> Result<(), Error> {

    let process_result = Command::new("nerdctl")
        .arg(namespace_arg)
        .arg("image")
        .arg("inspect")
        .arg(image_tag)
        .output();

    let has_image = match process_result {
        Ok(output) => output.status.success(),
        Err(_) => false
    };

    if has_image {
        return Ok(());
    }

    info!("Pulling container image {}", image_tag);
    Command::new("nerdctl")
        .arg(namespace_arg)
        .arg("image")
        .arg("pull")
        .arg(image_tag)
        .output()?;
    Ok(())
}
