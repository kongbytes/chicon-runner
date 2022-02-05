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

use anyhow::{Context, Error, Result};
use log::{info, error};
use tungstenite::{connect, WebSocket, stream::MaybeTlsStream};
use url::Url;

use config::Config;
use models::{CodeFunction, Scan};
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

            if let Err(text_error) = raw_message.to_text() {
                error!("Could not read text message from socket: {}", text_error);
                break;
            }
            let repository_id: &str = raw_message.to_text().unwrap();
            info!("Received request for repository ID {}", repository_id);

            let code_functions = scheduler.get_functions().context("Failure when retrieving functions")?;
            let repository = scheduler.get_repository(repository_id).context("Failure when retrieving repository")?;

            workspace.clean(&repository.public_id, false)?;

            info!("Starting function on repository {} with ID {} ({:?}, {:?})", repository.name, repository.public_id, repository.branch, repository.directory);
            let last_commit = repository.pull_or_clone(shared_config.clone())?;

            for code_function in code_functions.iter() {

                info!("Executing function \"{}\" in {} environment (ID {})", code_function.name, code_function.environment.name, code_function.public_id);

                let finished_scan = run_container(shared_config.clone(), &workspace, &repository.public_id , code_function, last_commit.clone())?;
                scheduler.store_scan(finished_scan)?;
            }

            workspace.clean(&repository.public_id, false)?;

            workspace.prune_storage()?;
        }
    }
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

    let mut metric_results: Option<HashMap<String, String>> = None;
    let potential_toml = workspace.read_string(repository_id, "result/data.toml");

    if let Ok(toml_content) = potential_toml {
        metric_results = Some(toml::from_str(&toml_content)?);
    }

    let finished_scan = Scan {
        function_id: code_function.public_id.to_string(),
        repository_id: repository_id.to_string(),
        commit,
        has_failed: !output.status.success() ,
        logs,
        timing_ms,
        results: metric_results.unwrap_or_default()
    };
    Ok(finished_scan)
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
