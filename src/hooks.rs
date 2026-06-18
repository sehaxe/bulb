use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{BulbError, Result};

#[derive(Debug, Clone, Default)]
pub struct InstallScript {
    pub pre_install: Option<String>,
    pub post_install: Option<String>,
    pub pre_upgrade: Option<String>,
    pub post_upgrade: Option<String>,
    pub pre_remove: Option<String>,
    pub post_remove: Option<String>,
}

impl InstallScript {
    pub fn parse(content: &str) -> Self {
        let mut script = InstallScript::default();
        let mut current_section = String::new();
        let mut current_content = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            let section_name = if trimmed.starts_with("pre_install()") {
                Some("pre_install")
            } else if trimmed.starts_with("post_install()") {
                Some("post_install")
            } else if trimmed.starts_with("pre_upgrade()") {
                Some("pre_upgrade")
            } else if trimmed.starts_with("post_upgrade()") {
                Some("post_upgrade")
            } else if trimmed.starts_with("pre_remove()") {
                Some("pre_remove")
            } else if trimmed.starts_with("post_remove()") {
                Some("post_remove")
            } else {
                None
            };

            if let Some(name) = section_name {
                if !current_content.is_empty() {
                    set_section(&mut script, &current_section, &current_content);
                }
                current_section = name.into();
                current_content.clear();
            } else if !current_section.is_empty() {
                if trimmed == "}" {
                    if !current_content.is_empty() {
                        set_section(&mut script, &current_section, &current_content);
                        current_section.clear();
                        current_content.clear();
                    }
                } else {
                    current_content.push_str(line);
                    current_content.push('\n');
                }
            }
        }

        if !current_section.is_empty() && !current_content.is_empty() {
            set_section(&mut script, &current_section, &current_content);
        }

        script
    }

    pub fn has_any(&self) -> bool {
        self.pre_install.is_some()
            || self.post_install.is_some()
            || self.pre_upgrade.is_some()
            || self.post_upgrade.is_some()
            || self.pre_remove.is_some()
            || self.post_remove.is_some()
    }

    pub fn run_pre(&self, pkg_name: &str, root: &Path, is_upgrade: bool) -> Result<()> {
        let script = if is_upgrade {
            self.pre_upgrade.as_deref().or(self.pre_install.as_deref())
        } else {
            self.pre_install.as_deref()
        };
        if let Some(s) = script {
            run_script(s, pkg_name, root)?;
        }
        Ok(())
    }

    pub fn run_post(&self, pkg_name: &str, root: &Path, is_upgrade: bool) -> Result<()> {
        let script = if is_upgrade {
            self.post_upgrade.as_deref().or(self.post_install.as_deref())
        } else {
            self.post_install.as_deref()
        };
        if let Some(s) = script {
            run_script(s, pkg_name, root)?;
        }
        Ok(())
    }
}

fn set_section(script: &mut InstallScript, section: &str, content: &str) {
    let content = content.trim().to_string();
    if content.is_empty() {
        return;
    }
    match section {
        "pre_install" => script.pre_install = Some(content),
        "post_install" => script.post_install = Some(content),
        "pre_upgrade" => script.pre_upgrade = Some(content),
        "post_upgrade" => script.post_upgrade = Some(content),
        "pre_remove" => script.pre_remove = Some(content),
        "post_remove" => script.post_remove = Some(content),
        _ => {}
    }
}

pub fn run_script(script: &str, pkg_name: &str, root: &Path) -> Result<()> {
    let wrapped = format!(
        r#"#!/bin/bash
export PKGNAME="{pkg_name}"
export ROOT="{root}"
export OP="install"

{}
"#,
        script,
        root = root.display()
    );

    let tmp = tempfile::tempdir()
        .map_err(|e| BulbError::Config(format!("failed to create temp dir: {e}")))?;
    let script_path = tmp.path().join("install.sh");
    fs::write(&script_path, &wrapped)?;

    let output = Command::new("bash")
        .arg(script_path)
        .current_dir(root)
        .output()
        .map_err(|e| BulbError::Config(format!("failed to run script: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BulbError::Config(format!(
            "install script failed for {pkg_name}: {stderr}"
        )));
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct PacnewFile {
    pub original: PathBuf,
    pub pacnew: PathBuf,
    pub package: String,
}

pub fn save_backup_files(root: &Path, backup_files: &[String]) -> Result<()> {
    for rel in backup_files {
        let dest = root.join(rel);
        if dest.exists() {
            let backup_path = dest.with_extension("user_backup");
            fs::copy(&dest, &backup_path)?;
        }
    }
    Ok(())
}

pub fn detect_user_modified(
    root: &Path,
    backup_files: &[String],
    old_files: &[PathBuf],
) -> std::collections::HashSet<PathBuf> {
    let mut modified = std::collections::HashSet::new();

    for rel in backup_files {
        let dest = root.join(rel);
        if !dest.exists() {
            continue;
        }
        if old_files.iter().any(|f| f == Path::new(rel)) {
            let _ = fs::read(&dest);
            modified.insert(rel.clone().into());
        }
    }

    modified
}

pub fn handle_pacnew(
    root: &Path,
    pkg_name: &str,
    backup_files: &[String],
    user_modified: &std::collections::HashSet<PathBuf>,
) -> Result<Vec<PacnewFile>> {
    let mut pacnew_files = Vec::new();

    for rel in backup_files {
        let dest = root.join(rel);
        if !dest.exists() {
            continue;
        }

        if user_modified.contains(Path::new(rel)) {
            let new_content = fs::read(&dest).unwrap_or_default();

            let user_backup = dest.with_extension("user_backup");
            if user_backup.exists() {
                fs::copy(&user_backup, &dest)?;
                fs::remove_file(&user_backup)?;
            }

            let pacnew_path = dest.with_extension("pacnew");
            fs::write(&pacnew_path, &new_content)?;

            pacnew_files.push(PacnewFile {
                original: dest,
                pacnew: pacnew_path,
                package: pkg_name.to_string(),
            });
        } else {
            let user_backup = dest.with_extension("user_backup");
            if user_backup.exists() {
                fs::remove_file(&user_backup)?;
            }
        }
    }

    Ok(pacnew_files)
}

pub fn handle_pacsave(
    root: &Path,
    pkg_name: &str,
    removed_files: &[PathBuf],
    backup_files: &[String],
) -> Result<Vec<PathBuf>> {
    let mut pacsave_files = Vec::new();

    for file in removed_files {
        let file_str = file.to_string_lossy().to_string();

        if backup_files.iter().any(|b| b == &file_str) {
            let dest = root.join(file);
            if dest.exists() {
                let pacsave_path = dest.with_extension("pacsave");
                fs::rename(&dest, &pacsave_path)?;
                pacsave_files.push(pacsave_path);
                println!("  saving {pkg_name}: {} as .pacsave", dest.display());
            }
        }
    }

    Ok(pacsave_files)
}

#[derive(Debug, Clone)]
pub struct SystemHook {
    pub name: String,
    pub description: Option<String>,
    pub operation: HookOperation,
    pub command: String,
    pub when: HookWhen,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookOperation {
    Install,
    Upgrade,
    Remove,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookWhen {
    PreTransaction,
    PostTransaction,
}

impl SystemHook {
    pub fn parse(content: &str, hook_name: &str) -> Vec<Self> {
        let mut hooks = Vec::new();
        let mut name = String::from(hook_name);
        let mut description = None;
        let mut when = HookWhen::PostTransaction;
        let mut exec = String::new();
        let mut operations = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                match key {
                    "Name" => name = value.to_string(),
                    "Description" => description = Some(value.to_string()),
                    "When" => {
                        when = match value {
                            "PreTransaction" => HookWhen::PreTransaction,
                            _ => HookWhen::PostTransaction,
                        };
                    }
                    "Exec" => exec = value.to_string(),
                    "Operation" | "Type" => {
                        match value {
                            "All" => {
                                operations = vec![
                                    HookOperation::Install,
                                    HookOperation::Upgrade,
                                    HookOperation::Remove,
                                ];
                            }
                            "Install" => operations.push(HookOperation::Install),
                            "Upgrade" => operations.push(HookOperation::Upgrade),
                            "Remove" => operations.push(HookOperation::Remove),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        if !exec.is_empty() {
            if operations.is_empty() {
                operations = vec![
                    HookOperation::Install,
                    HookOperation::Upgrade,
                    HookOperation::Remove,
                ];
            }

            for op in operations {
                hooks.push(SystemHook {
                    name: name.clone(),
                    description: description.clone(),
                    operation: op,
                    command: exec.clone(),
                    when: when.clone(),
                });
            }
        }

        hooks
    }
}

pub fn run_system_hooks(
    hooks_dir: &Path,
    pkg_name: &str,
    operation: HookOperation,
    when: HookWhen,
    root: &Path,
) -> Result<()> {
    if !hooks_dir.exists() {
        return Ok(());
    }

    let mut all_hooks = Vec::new();

    for entry in fs::read_dir(hooks_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("hook") {
            let content = fs::read_to_string(&path)?;
            let hook_name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
            let parsed = SystemHook::parse(&content, hook_name);
            all_hooks.extend(parsed);
        }
    }

    all_hooks.retain(|h| h.operation == operation && h.when == when);
    all_hooks.sort_by(|a, b| a.name.cmp(&b.name));

    for hook in &all_hooks {
        let cmd = hook.command.replace("%n", pkg_name);

        let wrapped = format!(
            r#"#!/bin/bash
export PKGNAME="{pkg_name}"
export ROOT="{root}"
{}
"#,
            cmd,
            root = root.display()
        );

        let tmp = tempfile::tempdir()?;
        let script_path = tmp.path().join("hook.sh");
        fs::write(&script_path, &wrapped)?;

        let output = Command::new("bash")
            .arg(script_path)
            .current_dir(root)
            .output()
            .map_err(|e| BulbError::Config(format!("hook {} failed: {e}", hook.name)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("warning: hook '{}' failed: {}", hook.name, stderr.trim());
        } else if let Some(desc) = &hook.description {
            println!("  {desc}");
        }
    }

    Ok(())
}

pub fn install_default_hooks(hooks_dir: &Path) -> Result<()> {
    fs::create_dir_all(hooks_dir)?;

    let modules_hook = r#"Name = 20-linux-modules-hook
Description = Loading new kernel modules...
Type = Install
Exec = test -x /usr/bin/depmod && /usr/bin/depmod -a %n
When = PostTransaction
"#;
    fs::write(hooks_dir.join("20-linux-modules-hook.hook"), modules_hook)?;

    Ok(())
}

pub fn default_hooks_dir(root: &Path) -> PathBuf {
    root.join("usr/share/bulb/hooks")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_install_script_with_braces() {
        let content = r#"pre_install() {
    echo "pre install"
}

post_install() {
    echo "post install"
}
"#;
        let script = InstallScript::parse(content);
        assert!(script.pre_install.is_some());
        assert!(script.post_install.is_some());
        assert!(script.pre_install.unwrap().contains("pre install"));
        assert!(script.post_install.unwrap().contains("post install"));
    }

    #[test]
    fn parses_install_script_without_trailing_newline() {
        let content = r#"pre_install() {
    echo "pre install"
}

post_install() {
    echo "post install"
}"#;
        let script = InstallScript::parse(content);
        assert!(script.pre_install.is_some());
        assert!(script.post_install.is_some());
    }

    #[test]
    fn parses_system_hook() {
        let content = r#"Name = booster
Description = Regenerating initramfs...
Type = Upgrade
Exec = /usr/bin/booster
When = PostTransaction
"#;
        let hooks = SystemHook::parse(content, "booster");
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].operation, HookOperation::Upgrade);
        assert_eq!(hooks[0].when, HookWhen::PostTransaction);
        assert_eq!(hooks[0].command, "/usr/bin/booster");
        assert_eq!(hooks[0].description.as_deref(), Some("Regenerating initramfs..."));
    }

    #[test]
    fn parses_hook_with_all_operations() {
        let content = r#"Name = test-hook
Type = All
Exec = /usr/bin/test %n
When = PostTransaction
"#;
        let hooks = SystemHook::parse(content, "test-hook");
        assert_eq!(hooks.len(), 3);
    }

    #[test]
    fn pacnew_detection() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/test.conf"), "old content").unwrap();

        let backup = vec!["etc/test.conf".to_string()];
        let old_files: Vec<PathBuf> = vec!["etc/test.conf".into()];

        let modified = detect_user_modified(root, &backup, &old_files);
        assert!(modified.contains(Path::new("etc/test.conf")));
    }
}
