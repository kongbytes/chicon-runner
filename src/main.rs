mod config;
mod cli;
mod models;
mod scheduler;
mod workspace;
mod utils;
mod commands;

use anyhow::{Error, Result};

use cli::build_cli;
use utils::assert_config_path;

fn main() -> Result<(), Error> {

    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("run", sub_matches)) => {

            let requested_config = sub_matches.get_one::<String>("config");
            let config_path = assert_config_path(requested_config);

            commands::run::launch_runner(config_path).unwrap();

        },
        Some(("check", sub_matches)) => {

            let requested_config = sub_matches.get_one::<String>("config");
            let config_path = assert_config_path(requested_config);

            commands::check::run_check(config_path);

        },
        _ => unreachable!()
    }

    Ok(())
}
