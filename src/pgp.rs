use std::path::Path;
use std::process::Command;

use crate::error::{BulbError, Result};

pub struct PgpVerifier {
    keyring_dir: Option<String>,
}

impl PgpVerifier {
    pub fn new() -> Self {
        Self { keyring_dir: None }
    }

    pub fn with_keyring(keyring_dir: &str) -> Self {
        Self {
            keyring_dir: Some(keyring_dir.to_string()),
        }
    }

    pub fn verify(&self, package_path: &Path, sig_path: &Path) -> Result<VerifyResult> {
        if !sig_path.exists() {
            return Ok(VerifyResult::NoSignature);
        }

        let mut cmd = Command::new("gpg");
        cmd.arg("--batch");
        cmd.arg("--status-fd");
        cmd.arg("1");
        cmd.arg("--verify");
        cmd.arg(sig_path);
        cmd.arg(package_path);

        if let Some(ref keyring) = self.keyring_dir {
            cmd.arg("--keyring");
            cmd.arg(format!("{}/pubring.gpg", keyring));
        }

        let output = cmd.output()
            .map_err(|e| BulbError::Signature(format!("failed to run gpg: {e}")))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        if output.status.success() {
            let key_id = extract_key_id(&stdout);
            Ok(VerifyResult::Valid { key_id })
        } else {
            if stderr.contains("No public key") {
                let key_id = extract_key_id(&stdout);
                return Err(BulbError::UnknownKey(
                    key_id.unwrap_or_else(|| "unknown".into())
                ));
            }
            Err(BulbError::Signature(format!(
                "verification failed: {}",
                stderr.lines().last().unwrap_or("unknown error")
            )))
        }
    }

    pub fn verify_detached(&self, package_path: &Path, signature: &[u8]) -> Result<VerifyResult> {
        let sig_path = package_path.with_extension("sig");
        std::fs::write(&sig_path, signature)?;
        let result = self.verify(package_path, &sig_path);
        let _ = std::fs::remove_file(&sig_path);
        result
    }

    pub fn import_key(&self, key_data: &[u8]) -> Result<()> {
        let mut cmd = Command::new("gpg");
        cmd.arg("--batch");
        cmd.arg("--import");

        if let Some(ref keyring) = self.keyring_dir {
            cmd.arg("--keyring");
            cmd.arg(format!("{}/pubring.gpg", keyring));
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| BulbError::Signature(format!("failed to run gpg: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(key_data)
                .map_err(|e| BulbError::Signature(format!("failed to write key: {e}")))?;
        }

        let output = child.wait_with_output()
            .map_err(|e| BulbError::Signature(format!("failed to wait for gpg: {e}")))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(BulbError::Signature(format!("key import failed: {stderr}")))
        }
    }
}

impl Default for PgpVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum VerifyResult {
    Valid { key_id: Option<String> },
    NoSignature,
    Expired { key_id: Option<String> },
}

fn extract_key_id(status_output: &str) -> Option<String> {
    for line in status_output.lines() {
        if let Some(rest) = line.strip_prefix("[GNUPG:] VALIDSIG ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(key_id) = parts.first() {
                return Some(key_id.to_string());
            }
        }
    }
    None
}

pub fn is_gpg_available() -> bool {
    Command::new("gpg")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gpg() {
        let available = is_gpg_available();
        println!("GPG available: {available}");
    }

    #[test]
    fn verify_missing_sig() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("test.pkg.tar.zst");
        std::fs::write(&pkg, b"test").unwrap();

        let sig = tmp.path().join("test.pkg.tar.zst.sig");
        let verifier = PgpVerifier::new();
        let result = verifier.verify(&pkg, &sig).unwrap();
        assert!(matches!(result, VerifyResult::NoSignature));
    }
}
