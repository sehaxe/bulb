use std::path::PathBuf;

use clap::{Parser, Subcommand};

use bulb::daemon::{self, DaemonConfig};
use bulb::error::Result;

#[derive(Parser)]
#[command(name = "bulbd", version, about = "bulb daemon")]
struct Cli {
    #[arg(long, default_value = "/run/bulbd.sock")]
    socket: PathBuf,

    #[arg(long, default_value = "/run/bulbd.pid")]
    pid: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/bulb.db")]
    db_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/content")]
    store_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/cache")]
    cache_path: PathBuf,

    #[arg(long, default_value_t = 2 * 1024 * 1024 * 1024)]
    max_cache_size: u64,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start,
    Stop,
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => {
            let config = DaemonConfig {
                socket_path: cli.socket,
                pid_path: cli.pid,
                db_path: cli.db_path,
                store_path: cli.store_path,
                cache_path: cli.cache_path,
                max_cache_size: cli.max_cache_size,
            };
            daemon::run_daemon(config).await
        }
        Commands::Stop => {
            if cli.pid.exists() {
                let pid_str = std::fs::read_to_string(&cli.pid)?;
                let pid: u32 = pid_str.trim().parse()
                    .map_err(|_| bulb::error::BulbError::Config("invalid PID file".into()))?;
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                println!("sent SIGTERM to bulbd (pid {pid})");
            } else {
                eprintln!("bulbd is not running (no PID file)");
            }
            Ok(())
        }
        Commands::Status => {
            if cli.pid.exists() {
                let pid_str = std::fs::read_to_string(&cli.pid)?;
                println!("bulbd running (pid {})", pid_str.trim());
            } else {
                println!("bulbd is not running");
            }
            Ok(())
        }
    }
}
