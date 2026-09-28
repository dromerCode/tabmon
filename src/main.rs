use std::process::ExitCode;

use clap::{Parser, Subcommand};

use tabmon::commands::{self, Ctx};
use tabmon::compositor::niri::Niri;
use tabmon::config::{Config, Paths};
use tabmon::error::render;
use tabmon::lock;
use tabmon::notify;
use tabmon::system::RealSystem;

/// Use an Android tablet as an extended monitor on niri.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Print every external command before running it
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Clone, Copy)]
enum Cmd {
    /// Turn on the virtual output and start the connection
    On,
    /// Stop everything tabmon started
    Off,
    /// Turn on or off depending on the current state (for a keybinding)
    Toggle,
    /// Show whether it is running, and on which output, port and device
    Status,
    /// Check the setup and explain how to fix what is missing
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let sys = RealSystem {
        verbose: cli.verbose,
    };
    let setup = Paths::from_env().and_then(|paths| Ok((Config::load(&paths.config_file)?, paths)));
    // Without a readable config, notify anyway: that is the default.
    let notify_errors = match &setup {
        Ok((config, _)) => config.notify,
        Err(_) => true,
    };

    match setup.and_then(|(config, paths)| run(cli.command, &sys, config, paths)) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{}", notify::failure(&sys, &err, notify_errors));
            ExitCode::FAILURE
        }
    }
}

fn run(command: Cmd, sys: &RealSystem, config: Config, paths: Paths) -> anyhow::Result<ExitCode> {
    // Commands that change the session run one at a time; a second keypress is dropped,
    // without a notification, because the first one already shows its own.
    let _lock = match command {
        Cmd::On | Cmd::Off | Cmd::Toggle => match lock::try_acquire(&paths)? {
            Some(lock) => Some(lock),
            None => {
                eprintln!("{}", render(&lock::busy().into()));
                return Ok(ExitCode::FAILURE);
            }
        },
        Cmd::Status | Cmd::Doctor => None,
    };
    let niri = Niri::new(sys);
    let ctx = Ctx {
        sys,
        compositor: &niri,
        config,
        paths,
    };
    let message = match command {
        Cmd::On => commands::on::run(&ctx)?,
        Cmd::Off => commands::off::run(&ctx)?,
        Cmd::Toggle => commands::toggle::run(&ctx)?,
        Cmd::Status => commands::status::run(&ctx)?,
        Cmd::Doctor => {
            let report = commands::doctor::run(&ctx);
            println!("{}", report.render());
            return Ok(if report.ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
    };
    println!("{message}");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn verbose_works_after_the_subcommand() {
        let cli = Cli::try_parse_from(["tabmon", "on", "-v"]).unwrap();
        assert!(cli.verbose);
        assert!(matches!(cli.command, Cmd::On));
    }
}
