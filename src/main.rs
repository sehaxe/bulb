mod commands;

use clap::Parser;

fn main() {
    if let Err(err) = commands::run(commands::Cli::parse()) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
