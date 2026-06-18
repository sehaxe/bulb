mod commands;

use clap::Parser;

fn main() {
    if let Err(err) = commands::run(commands::Cli::parse()) {
        let code = err.exit_code();
        eprintln!("error: {err:#}");
        std::process::exit(code);
    }
}
