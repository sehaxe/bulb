use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{BulbError, Result};

const LOCK_DIR: &str = "/var/lib/bulb";

pub struct Lock {
    file: File,
    path: PathBuf,
}

impl Lock {
    pub fn acquire(root: &Path) -> Result<Self> {
        let lock_dir = root.join(LOCK_DIR.trim_start_matches('/'));
        fs::create_dir_all(&lock_dir)?;

        let lock_path = lock_dir.join("bulb.lock");
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&lock_path)
            .map_err(|e| BulbError::Config(format!("failed to open lock: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            unsafe {
                if libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) != 0 {
                    let err = std::io::Error::last_os_error();
                    if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                        return Err(BulbError::Config(
                            "another bulb instance is running. wait for it to finish.".into(),
                        ));
                    }
                    return Err(BulbError::Config(format!("failed to acquire lock: {err}")));
                }
            }
        }

        let _ = file.set_len(0);
        write!(&file, "{}", std::process::id())?;

        Ok(Lock { file, path: lock_path })
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self.file.as_raw_fd();
            unsafe {
                libc::flock(fd, libc::LOCK_UN);
            }
        }
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        {
            let _lock = Lock::acquire(root).unwrap();
        }
        let lock2 = Lock::acquire(root);
        assert!(lock2.is_ok());
    }

    #[test]
    fn concurrent_lock_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let _lock1 = Lock::acquire(root).unwrap();
        let lock2 = Lock::acquire(root);
        assert!(lock2.is_err());
    }
}
