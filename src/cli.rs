use clap::{Arg, Command};

pub fn build_cli() -> Command {

    Command::new("chicon-runner")
        .about("Chicon code runner")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("run")
                .about("Start the runner in server mode")
                .arg(
                    Arg::new("config")
                        .short('c')
                        .long("config")
                        .help("Config file path")
                )
        )
        .subcommand(
            Command::new("check")
                .about("Perform a runner health check")
                .arg(
                    Arg::new("config")
                        .short('c')
                        .long("config")
                        .help("Config file path")
                )
        )
}
