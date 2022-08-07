mod config;
mod cli;
mod models;
mod scheduler;
mod workspace;
mod utils;
mod commands;

use std::process;

use anyhow::{Error, Result};

use cli::build_cli;

fn main() -> Result<(), Error> {

    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("run", sub_matches)) => {

            let config_path = sub_matches.get_one::<String>("config").unwrap_or_else(|| {
                eprint!("Could not extract configuration path");
                process::exit(1);
            });
            commands::run::launch_runner(config_path).unwrap();

        },
        Some(("check", _sub_matches)) => {

            commands::check::run_check();

        }
        _ => unreachable!()
    }

    Ok(())
}
