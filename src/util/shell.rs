use std::path::Path;
use std::process::Command;

use crate::error::{BulbError, Result};

pub fn run_script(script: &str, root: &Path) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(root)
        .status()?;
    if !status.success() {
        return Err(BulbError::InvalidMetadata(format!(
            "script exited with {status}"
        )));
    }
    Ok(())
}
