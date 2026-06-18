use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;

use crate::core::pkginfo::PackageInfo;
use crate::db::store::ContentStore;
use crate::db::Database;
use crate::error::{BulbError, Result};

pub struct InstallPlan {
    pub packages: Vec<QueuedPackage>,
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub store_path: PathBuf,
    pub staging_dir: PathBuf,
}

pub struct QueuedPackage {
    pub path: PathBuf,
    pub info: Option<PackageInfo>,
}

pub struct InstallResult {
    pub installed: Vec<String>,
    pub errors: Vec<String>,
}

impl InstallPlan {
    pub fn new(root: PathBuf, db_path: PathBuf, store_path: PathBuf) -> Result<Self> {
        let staging_dir = tempfile::tempdir()?.keep();
        Ok(Self {
            packages: Vec::new(),
            root,
            db_path,
            store_path,
            staging_dir,
        })
    }

    pub fn queue(&mut self, package: PathBuf) {
        self.packages.push(QueuedPackage {
            path: package,
            info: None,
        });
    }

    pub fn execute(self) -> Result<InstallResult> {
        let store = Arc::new(ContentStore::new(self.store_path.clone()));
        store.init()?;

        let mut db = Database::open(&self.db_path)?;
        let gen_id = db.ensure_generation()?;

        let mut installed = Vec::new();
        let mut errors = Vec::new();

        let extracted: Vec<Result<(PackageInfo, Vec<PathBuf>)>> = self
            .packages
            .par_iter()
            .map(|pkg| extract_package(&pkg.path, &self.staging_dir, &store))
            .collect();

        for (pkg, result) in self.packages.iter().zip(extracted) {
            match result {
                Ok((info, files)) => {
                    if let Some(owner) = db.find_file_owner(gen_id, &info.name)? {
                        errors.push(format!("{}: conflict with {}", info.name, owner));
                        continue;
                    }

                    let new_gen = db.create_generation(&format!("install {}", info.name))?;
                    db.insert_installed_package(
                        new_gen,
                        &info,
                        &files,
                        &format!("installed-{}", info.name),
                    )?;

                    let staging_pkg = self.staging_dir.join(&info.name);
                    if staging_pkg.exists() {
                        apply_staging(&staging_pkg, &self.root)?;
                    }

                    installed.push(format!("{} {}", info.name, info.version));
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", pkg.path.display()));
                }
            }
        }

        fs::remove_dir_all(&self.staging_dir).ok();

        Ok(InstallResult {
            installed,
            errors,
        })
    }
}

fn extract_package(
    package: &Path,
    staging: &Path,
    store: &Arc<ContentStore>,
) -> Result<(PackageInfo, Vec<PathBuf>)> {
    let file_name = package.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let (info, files) = if file_name.ends_with(".pkg.tar.zst") {
        let file = fs::File::open(package)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let decoder = zstd::stream::Decoder::new(&mmap[..])?;
        let mut archive = tar::Archive::new(decoder);
        single_pass_extract(&mut archive, staging, store)?
    } else {
        return Err(BulbError::UnsupportedPackageFormat(package.to_path_buf()));
    };

    Ok((info, files))
}

fn single_pass_extract<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    staging: &Path,
    store: &ContentStore,
) -> Result<(PackageInfo, Vec<PathBuf>)> {
    let mut pkginfo_text = None;
    let mut files = Vec::new();
    let mut created_dirs = std::collections::HashSet::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let file_name = entry_path.file_name().and_then(|n| n.to_str());

        match file_name {
            Some(".PKGINFO") => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                pkginfo_text = Some(text);
            }
            Some(".BUILDINFO") | Some("install") | Some(".MTREE") => {}
            _ => {
                let relative = match normalize_path(&entry_path) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if relative.as_os_str().is_empty() {
                    continue;
                }

                let dest = staging.join(&relative);
                match entry.header().entry_type() {
                    tar::EntryType::Directory => {
                        if created_dirs.insert(dest.clone()) {
                            let _ = fs::create_dir_all(&dest);
                        }
                    }
                    tar::EntryType::Regular => {
                        ensure_parent_dir(&dest, staging, &mut created_dirs)?;
                        let mut data = Vec::new();
                        entry.read_to_end(&mut data)?;
                        let hash = store.add(&data)?;
                        store.link(&hash, &dest)?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            if let Ok(mode) = entry.header().mode() {
                                let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(mode));
                            }
                        }
                    }
                    tar::EntryType::Symlink => {
                        ensure_parent_dir(&dest, staging, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let _ = fs::remove_file(&dest);
                            #[cfg(unix)]
                            std::os::unix::fs::symlink(&link_target, &dest)?;
                        }
                    }
                    tar::EntryType::Link => {
                        ensure_parent_dir(&dest, staging, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let link_dest = staging.join(&link_target);
                            let _ = fs::remove_file(&dest);
                            fs::hard_link(&link_dest, &dest)?;
                        }
                    }
                    _ => continue,
                }
                files.push(relative);
            }
        }
    }

    let pkginfo_text = pkginfo_text.ok_or_else(|| {
        BulbError::InvalidMetadata("archive missing .PKGINFO".into())
    })?;
    let pkginfo = crate::format::alpm::pkginfo::PkgInfo::parse(&pkginfo_text);
    let info = crate::format::alpm::convert::package_info_from_pkginfo(&pkginfo);

    Ok((info, files))
}

fn apply_staging(staging_pkg: &Path, root: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(staging_pkg) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(staging_pkg)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let dest = root.join(relative);
        if entry.file_type().is_dir() {
            let _ = fs::create_dir_all(&dest);
        } else if entry.file_type().is_file() {
            ensure_parent_dir(&dest, root, &mut std::collections::HashSet::new())?;
            if dest.exists() || dest.symlink_metadata().is_ok() {
                fs::remove_file(&dest)?;
            }
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn ensure_parent_dir(
    path: &Path,
    root: &Path,
    created_dirs: &mut std::collections::HashSet<PathBuf>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if parent != root && !created_dirs.contains(parent) {
            let mut current = parent.to_path_buf();
            let mut stack = Vec::new();
            while current != root && !created_dirs.contains(&current) {
                stack.push(current.clone());
                match current.parent() {
                    Some(p) if p != current => current = p.to_path_buf(),
                    _ => break,
                }
            }
            for dir in stack.into_iter().rev() {
                if created_dirs.insert(dir.clone()) {
                    let _ = fs::create_dir_all(&dir);
                }
            }
        }
    }
    Ok(())
}

fn normalize_path(path: &Path) -> Result<PathBuf> {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(BulbError::UnsafeArchivePath(path.display().to_string()));
            }
        }
    }
    Ok(normalized)
}

pub fn needs_root(operations: &[&str]) -> bool {
    operations.iter().any(|op| match *op {
        "switch-generation" | "rollback" | "apply-staging" => true,
        _ => false,
    })
}

pub fn ask_root_once(operations: &[&str]) -> Result<bool> {
    if !needs_root(operations) {
        return Ok(true);
    }

    eprintln!("The following operations require root privileges:");
    for op in operations {
        eprintln!("  - {op}");
    }
    eprintln!("\nRun with: sudo bulb {}", operations.join(" "));

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_root() {
        assert!(needs_root(&["switch-generation"]));
        assert!(needs_root(&["rollback"]));
        assert!(!needs_root(&["install"]));
        assert!(!needs_root(&["query"]));
    }

    #[test]
    fn test_install_plan_queue() {
        let dir = tempfile::tempdir().unwrap();
        let mut plan = InstallPlan::new(
            dir.path().join("root"),
            dir.path().join("db"),
            dir.path().join("store"),
        )
        .unwrap();

        plan.queue(PathBuf::from("/tmp/test.pkg.tar.zst"));
        assert_eq!(plan.packages.len(), 1);
    }
}
