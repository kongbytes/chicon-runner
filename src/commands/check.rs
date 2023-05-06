use std::process::{Command, Stdio};
use std::process;

use log::error;

use crate::config::Config;

pub fn run_check(config_path: &str) {

    println!();
    println!("Starting Chicon runner health checks");
    println!();

    let config = Config::parse(config_path).unwrap_or_else(|err| {
        error!("FAIL, could not read or parse config file ({})", err);
        process::exit(1);
    });
    println!("OK, valid configuration file found");

    let git_result = Command::new("git")
        .arg("version")
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let git_exit_status = git_result.unwrap_or_else(|err| {
        error!("FAIL, could not launch the 'git' binary and execute a 'version' command ({})", err);
        process::exit(1);
    });
    if !git_exit_status.success() {
        error!("FAIL, could not launch the 'git' binary and execute a 'version' command (status {})", git_exit_status);
        process::exit(1);
    }
    println!("OK, git binary launched");

    let namespace_arg = format!("--namespace={}", config.container.namespace);
    let process_result = Command::new("nerdctl")
        .arg(namespace_arg)
        .arg("ps")
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let exit_status = process_result.unwrap_or_else(|err| {
        error!("FAIL, could not launch the 'nerdctl' binary and execute a 'ps' command ({})", err);
        process::exit(1);
    });
    if !exit_status.success() {
        error!("FAIL, could not launch the 'nerdctl' binary and execute a 'ps' command (status {})", exit_status);
        process::exit(1);
    }
    println!("OK, nerdctl binary launched");

    println!();

}
