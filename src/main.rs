mod config;
mod models;
mod scheduler;
mod workspace;
mod utils;

use std::collections::HashMap;
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Error, Result};
use log::{info};
use tungstenite::{connect};
use url::Url;

use config::Config;
use models::{CodeFunction, Scan};
use scheduler::Scheduler;
use workspace::Workspace;

fn main() -> Result<(), Error> {

    env_logger::init();

    let config = Config::parse("./data/config.toml")?;
    let workspace = Workspace::new(&config);
    let scheduler = Scheduler::new(&config);

    let scheduler_raw_url = format!("ws://{}/ws/runners", config.scheduler);
    let scheduler_url = Url::parse(&scheduler_raw_url)?;
    let (mut socket, _response) = connect(scheduler_url)?;
    info!("Connected to the scheduler, ready to receive requests");

    loop {

        let raw_message = socket.read_message()?;
        let repository_id: &str = raw_message.to_text()?;
        info!("Received request for repository ID {}", repository_id);

        let code_functions = scheduler.get_functions().context("Failure when retrieving functions")?;
        let repository = scheduler.get_repository(repository_id).context("Failure when retrieving repository")?;

        // TODO Repo caching can be more optimal
        workspace.clean(true)?;

        info!("Starting function on repository {} with ID {} ({:?}, {:?})", repository.name, repository.public_id, repository.branch, repository.directory);
        repository.clone(&config)?;

        for code_function in code_functions.iter() {

            let finished_scan = run_container(&config, &workspace, &repository.public_id , code_function)?;
            scheduler.store_scan(finished_scan)?;
        }

        workspace.clean(true)?;
    }
}

fn run_container(config: &Config, workspace: &Workspace, repository_id: &str, code_function: &CodeFunction) -> Result<Scan, Error> {

    info!("Executing function \"{}\" in {} environment (ID {})", code_function.name, code_function.environment.name, code_function.public_id);
    workspace.clean(false)?;

    let script_path = format!("bin/process.{}", &code_function.environment.file_extension);
    workspace.write_string(&script_path, &code_function.content)?;

    let mut nerdctl = Command::new("nerdctl");
    nerdctl
        .arg("--namespace=kb")
        .arg("run")
        .arg("--rm")
        // Security flags
        .arg("--cap-drop")
        .arg("all")
        .arg("--security-opt")
        .arg("apparmor=docker-default");

    if code_function.capabilities.network {
        nerdctl.arg("--network").arg("bridge");
    }
    else {
        nerdctl.arg("--network").arg("none");
    }
    
    nerdctl.arg("--volume") // Volume mounting
        .arg(format!("{}/repository:/workspace:ro", config.workspace))
        .arg("--volume")
        .arg(format!("{}/bin:/tmp-bin:ro", config.workspace))
        .arg("--volume")
        .arg(format!("{}/result:/result", config.workspace))
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
    let potential_toml = workspace.read_string("result/data.toml");

    if let Ok(toml_content) = potential_toml {
        metric_results = Some(toml::from_str(&toml_content)?);
    }

    let finished_scan = Scan {
        function_id: code_function.public_id.to_string(),
        repository_id: repository_id.to_string(),
        has_failed: !output.status.success() ,
        logs,
        timing_ms,
        results: metric_results.unwrap_or_default()
    };
    Ok(finished_scan)
}
