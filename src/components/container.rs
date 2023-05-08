use std::{process::Command, time::SystemTime, collections::HashMap, rc::Rc};

use anyhow::{Context, Error, Result};
use log::info;

use crate::models::{CodeFunction, ScanMetadata, Scan, GitCommit};

use super::{workspace::Workspace, config::Config};

pub fn pull_image(image_tag: &str, namespace_arg: &str) -> Result<(), Error> {

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

pub fn run_container(config: Rc<Config>, workspace: &Workspace, repository_id: &str, code_function: &CodeFunction, commit: GitCommit) -> Result<Scan, Error> {

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