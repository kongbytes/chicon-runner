mod config;
mod models;
mod scheduler;
mod workspace;
mod utils;

use std::collections::HashMap;
use std::process::Command;
use std::time::{SystemTime, Duration};
use std::rc::Rc;
use std::net::TcpStream;
use std::thread;

use anyhow::{bail, Context, Error, Result};
use log::{info, error};
use tungstenite::{connect, WebSocket, stream::MaybeTlsStream};
use url::Url;
use serde::Deserialize;

use config::Config;
use models::{CodeFunction, Scan, ScanMetadata, CodeIssue};
use scheduler::Scheduler;
use workspace::Workspace;

fn main() -> Result<(), Error> {

    env_logger::init();

    let config = Config::parse("./data/config.toml")?;
    let shared_config = Rc::new(config);
    let workspace = Workspace::new(shared_config.clone());
    let scheduler = Scheduler::new(shared_config.clone());

    let storage_mb = workspace.get_total_usage()? / 1_000_000;
    info!("Workspace usage is currently {}Mb ({}Mb limit)", storage_mb, shared_config.workspace.cache_limit);

    let websocker_raw_url = format!("ws://{}/ws/runners", shared_config.scheduler.base_url);
    let websocket_url = Url::parse(&websocker_raw_url)?;

    loop {

        let mut some_websocket: Option<WebSocket<MaybeTlsStream<TcpStream>>> = None;

        // Perform scheduler connect attempts in this loop, this enables the
        // runner to be more resilient against scheduler crashes.
        loop {

            let mut is_connected = false;

            match connect(&websocket_url) {
                Ok((socket, _response)) => {

                    some_websocket = Some(socket);
                    is_connected = true;

                },
                Err(err) => {

                    error!("Scheduler connection failed: {}", err);
                    error!("Retry in {} seconds", shared_config.scheduler.retry_period);

                    let duration = Duration::from_secs(shared_config.scheduler.retry_period);
                    thread::sleep(duration);

                }
            };

            if is_connected {
                break;
            }
        }

        info!("Connected to the scheduler, ready to receive requests");
        let mut websocket = some_websocket.unwrap();

        loop {

            let socket_read_result = websocket.read_message();

            if let Err(read_error) = socket_read_result {

                error!("Failed to read messages from scheduler: {}", read_error);

                let duration = Duration::from_secs(shared_config.scheduler.retry_period);
                thread::sleep(duration);

                break;
            }
            let raw_message = socket_read_result.unwrap();

            let decode_result = decode_message(raw_message);
            if decode_result.is_err() {
                error!("Could not read text message from socket");
                break;   
            }
            let request = decode_result.unwrap();
            info!("Received request for repository ID {}", request.repository_id);

            let code_functions = scheduler.get_functions(&request.functions).context("Failure when retrieving functions")?;
            let repository = scheduler.get_repository(&request.repository_id).context("Failure when retrieving repository")?;

            workspace.clean(&repository.public_id, false)?;

            info!("Starting function on repository {} with ID {} ({:?}, {:?})", repository.name, repository.public_id, repository.branch, repository.directory);
            let last_commit = repository.pull_or_clone(shared_config.clone())?;

            for code_function in code_functions.iter() {

                info!("Executing function \"{}\" in {} environment (ID {})", code_function.name, code_function.environment.name, code_function.public_id);

                let finished_scan = run_container(shared_config.clone(), &workspace, &repository.public_id , code_function, last_commit.clone())?;
                scheduler.store_scan(finished_scan)?;
                process_issues(&workspace, &repository.public_id, &scheduler)?;
            }

            workspace.clean(&repository.public_id, false)?;
            workspace.prune_storage()?;
        }
    }
}

struct ScanRequest {
    _version: String,
    repository_id: String,
    functions: Vec<String>
}

fn decode_message(raw_message: tungstenite::Message) -> Result<ScanRequest, Error> {

    if let Err(text_error) = raw_message.to_text() {
        bail!(text_error);
    }

    // TODO Make messsage parse below (v1;repos;functions) more effective
    // and resilient against crashes (does not handle multiple repos, version
    // control and function selection).
    let runner_command: &str = raw_message.to_text().unwrap();
    let message_parts: Vec<&str> = runner_command.split(';').collect();
    let repository_id = message_parts.get(1).unwrap();

    let scan_request = ScanRequest {
        _version: message_parts.get(0).unwrap_or(&"v1").to_string(),
        repository_id: repository_id.to_string(),
        functions: vec![]
    };
    Ok(scan_request)
}

fn run_container(config: Rc<Config>, workspace: &Workspace, repository_id: &str, code_function: &CodeFunction, commit: models::GitCommit) -> Result<Scan, Error> {

    workspace.clean(repository_id, false).context("Could not clean workspace before run")?;

    let script_path = format!("bin/process.{}", &code_function.environment.file_extension);
    workspace.write_string(repository_id, &script_path, &code_function.content)?;

    let namespace_arg = format!("--namespace={}", config.container.namespace);
    pull_image(&code_function.environment.base_image, &namespace_arg).context("Could not pull container image")?;

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
    
    if let Some(user) = &code_function.environment.user {
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
    nerdctl.arg(&code_function.environment.base_image)
        .arg(&code_function.environment.executor)
        .arg(format!("/tmp-bin/process.{}", &code_function.environment.file_extension));
    
    let start_time = SystemTime::now();
    let output = nerdctl.output()?;
    let timing_ms: usize = utils::compute_time_diff(start_time)?;

    let stderr_logs = String::from_utf8(output.stderr).unwrap_or_else(|_| "(invalid UTF8 string)".to_string());
    let stdout_logs = String::from_utf8(output.stdout).unwrap_or_else(|_| "(invalid UTF8 string)".to_string());
    let logs = format!("{}\n{}", stdout_logs, stderr_logs);

    let mut metric_results: Option<HashMap<String, models::MetricValue>> = None;
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
        has_failed: !output.status.success() ,
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

fn process_issues(workspace: &Workspace, repository_id: &str, scheduler: &Scheduler) -> Result<(), Error> {

    let potential_issues = workspace.read_string(repository_id, "result/issues.toml");

    if let Ok(vulnerabilities_content) = potential_issues {

        let mut issue_list: Vec<CodeIssue> = vec![];
        let parsed_toml: Result<IssueContainer, toml::de::Error> = toml::from_str(&vulnerabilities_content);

        if let Ok(toml_results) = parsed_toml {
            issue_list = toml_results.issues;
        }
        else {
            // TODO TOML error handling
            error!("Could not parse issue TOML for repository {}", repository_id);
        }

        for issue_item in issue_list {

            scheduler.store_issue(CodeIssue {
                name: issue_item.name,
                repository_id: Some(repository_id.to_string())
            })?;
        }
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
