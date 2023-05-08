use std::process::{Command, Stdio};
use std::process;

use log::error;

use crate::components::config::Config;

pub fn run_check(config_path: Option<&str>) {

    println!();
    println!("Starting Chicon runner health checks");

    let config = match config_path {
        Some(path) => {
    
            Config::parse(path).unwrap_or_else(|err| {
                error!("FAIL, could not read or parse config file ({})", err);
                process::exit(1);
            })
  
        },
        None => {
            println!("Using default configuration - this is not recommended for production");
            Config::default()
        }
    };
    
    println!();
    println!("OK, runner configuration is valid");

    check_git_binary();
    check_nerdctl_binary(&config);
    
    println!();

}

fn check_git_binary() {

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
}

fn check_nerdctl_binary(config: &Config) {

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
}
