use std::path::PathBuf;
use std::process::Command;

use crate::error::{BulbError, Result};

pub struct SandboxConfig {
    pub source_dir: PathBuf,
    pub output: PathBuf,
    pub allow_network: bool,
    pub extra_ro: Vec<PathBuf>,
    pub extra_rw: Vec<PathBuf>,
}

impl SandboxConfig {
    pub fn new(source_dir: PathBuf, output: PathBuf) -> Self {
        Self {
            source_dir,
            output,
            allow_network: false,
            extra_ro: Vec::new(),
            extra_rw: Vec::new(),
        }
    }
}

pub struct SandboxRunner;

impl SandboxRunner {
    pub fn is_available() -> bool {
        Command::new("which")
            .arg("bwrap")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn run(config: &SandboxConfig) -> Result<PathBuf> {
        if !Self::is_available() {
            return Err(BulbError::Sandbox(
                "bubblewrap (bwrap) not found — install with: pacman -S bubblewrap".into(),
            ));
        }

        let output_dir = config
            .output
            .parent()
            .unwrap_or(&config.output)
            .to_path_buf();

        let mut args: Vec<String> = Vec::new();

        args.push("--unshare-pid".into());
        args.push("--unshare-uts".into());
        args.push("--unshare-ipc".into());

        if !config.allow_network {
            args.push("--unshare-net".into());
        }

        args.push("--die-with-parent".into());

        args.push("--ro-bind".into());
        args.push("/".into());
        args.push("/".into());

        args.push("--bind".into());
        args.push(config.source_dir.to_string_lossy().into_owned());
        args.push(config.source_dir.to_string_lossy().into_owned());

        args.push("--bind".into());
        args.push(output_dir.to_string_lossy().into_owned());
        args.push(output_dir.to_string_lossy().into_owned());

        for extra in &config.extra_ro {
            args.push("--ro-bind".into());
            args.push(extra.to_string_lossy().into_owned());
            args.push(extra.to_string_lossy().into_owned());
        }
        for extra in &config.extra_rw {
            args.push("--bind".into());
            args.push(extra.to_string_lossy().into_owned());
            args.push(extra.to_string_lossy().into_owned());
        }

        args.push("--tmpfs".into());
        args.push("/tmp".into());

        args.push("--proc".into());
        args.push("/proc".into());

        args.push("--dev".into());
        args.push("/dev".into());

        let build_cmd = format!(
            "cd {} && bulb build . --output {}",
            config.source_dir.display(),
            config.output.display(),
        );

        args.push("--".into());
        args.push("sh".into());
        args.push("-c".into());
        args.push(build_cmd);

        let status = Command::new("bwrap")
            .args(&args)
            .status()
            .map_err(|e| BulbError::Sandbox(format!("failed to exec bwrap: {e}")))?;

        if !status.success() {
            return Err(BulbError::Sandbox(format!(
                "sandbox build failed with exit code: {}",
                status.code().unwrap_or(-1)
            )));
        }

        if config.output.exists() {
            Ok(config.output.clone())
        } else {
            Err(BulbError::Sandbox(
                "build completed but output file not found".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_config_defaults() {
        let config = SandboxConfig::new(PathBuf::from("/src"), PathBuf::from("/out/pkg.tar.zst"));
        assert!(!config.allow_network);
        assert!(config.extra_ro.is_empty());
        assert!(config.extra_rw.is_empty());
    }

    #[test]
    fn is_available_returns_bool() {
        let _ = SandboxRunner::is_available();
    }

    #[test]
    fn build_args_unshare_pid() {
        let config = SandboxConfig::new(PathBuf::from("/src"), PathBuf::from("/out/pkg.tar.zst"));
        assert!(!config.allow_network);
    }
}
