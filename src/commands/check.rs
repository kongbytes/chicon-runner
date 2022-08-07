use std::process::{Command, Stdio};
use std::process;

use log::error;

use crate::config::Config;

pub fn run_check() {

    // TODO
    let config = Config::parse("./data/config.toml").unwrap_or_else(|err| {
        error!("Could not read or parse config file ({})", err);
        process::exit(1);
    });
    
    let namespace_arg = format!("--namespace={}", config.container.namespace);
    let process_result = Command::new("nerdctl")
        .arg(namespace_arg)
        .arg("ps")
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Should run nerdctl command");

    if process_result.success() {
        println!("OK");
    }
    else {
        println!("Fail");
    }
}
