mod cli;
mod models;
mod utils;
mod commands;
mod components;

use std::process;

use anyhow::{Error, Result};

use cli::build_cli;
use utils::find_default_config;

fn main() -> Result<(), Error> {

    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("run", sub_matches)) => {

            let requested_config = sub_matches.get_one::<String>("config");
            let config_path = find_default_config(requested_config);

            commands::run::launch_runner(config_path).unwrap_or_else(|err| {
                eprintln!("Chicon runner process failed due to fatal error");
                eprintln!("{}", err);
                process::exit(1);
            });

        },
        Some(("check", sub_matches)) => {

            let requested_config = sub_matches.get_one::<String>("config");
            let config_path = find_default_config(requested_config);

            commands::check::run_check(config_path);

        },
        _ => unreachable!()
    }

    Ok(())
}
