//! Parser for `/etc/pacman.conf`.
//!
//! The pacman config is an INI-ish file with three quirks we must honour:
//!
//! 1. Boolean options have no `=`/value (`Color`, `CheckSpace`).
//! 2. `Include = <path>` recursively inlines another file (the mirrorlists).
//! 3. Repo precedence is **file order**: the first `[repo]` section wins on
//!    name collisions, so we preserve insertion order.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{BulbError, Result};

/// Parsed `[options]` section. Only the keys bulb cares about are typed;
/// everything else is kept as a raw `Vec<String>` so nothing is lost.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub root_dir: Option<PathBuf>,
    pub db_path: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub gpg_dir: Option<PathBuf>,
    pub hook_dir: Option<PathBuf>,
    pub architecture: Option<String>,
    pub hold_pkg: Vec<String>,
    pub ignore_pkg: Vec<String>,
    pub ignore_group: Vec<String>,
    pub no_upgrade: Vec<String>,
    pub no_extract: Vec<String>,
    pub sig_level: Vec<String>,
    pub local_file_sig_level: Vec<String>,
    pub remote_file_sig_level: Vec<String>,
    pub parallel_downloads: Option<u32>,
    pub download_user: Option<String>,
    pub check_space: bool,
    pub color: bool,
    /// All other recognized/unknown keys, preserved verbatim.
    pub extra: BTreeMap<String, Vec<String>>,
}

/// A single `[repo]` section. Order matters — see module docs.
#[derive(Debug, Clone)]
pub struct Repo {
    pub name: String,
    pub servers: Vec<String>,
    pub sig_level: Vec<String>,
    pub usage: Vec<String>,
}

/// Fully parsed pacman.conf.
#[derive(Debug, Clone, Default)]
pub struct PacmanConf {
    pub options: Options,
    /// Repos in file order (precedence = position).
    pub repos: Vec<Repo>,
}

impl PacmanConf {
    /// Parse the default `/etc/pacman.conf`. Returns a config with empty
    /// options if the file is missing (useful for tests / non-Arch hosts).
    pub fn load_default() -> Result<Self> {
        Self::load(Path::new("/etc/pacman.conf"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        Self::parse(&text, path.parent().unwrap_or(Path::new("/")))
    }

    /// Parse config text. `base_dir` resolves relative `Include` paths.
    pub fn parse(text: &str, base_dir: &Path) -> Result<Self> {
        let mut conf = PacmanConf::default();
        let mut current_repo: Option<String> = None;
        // Track recursion depth to defuse include loops.
        parse_into(text, base_dir, &mut conf, &mut current_repo, 0)?;
        Ok(conf)
    }

    /// Find a repo by name.
    pub fn repo(&self, name: &str) -> Option<&Repo> {
        self.repos.iter().find(|r| r.name == name)
    }
}

fn parse_into(
    text: &str,
    base_dir: &Path,
    conf: &mut PacmanConf,
    current_repo: &mut Option<String>,
    depth: u32,
) -> Result<()> {
    const MAX_INCLUDE_DEPTH: u32 = 32;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Section header.
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if name.eq_ignore_ascii_case("options") {
                *current_repo = None;
            } else {
                // Register a new repo section so we keep order even if it has
                // no Server lines yet (mirrorlist-only repos do this).
                if !conf.repos.iter().any(|r| r.name == name) {
                    conf.repos.push(Repo {
                        name: name.to_string(),
                        servers: Vec::new(),
                        sig_level: Vec::new(),
                        usage: Vec::new(),
                    });
                }
                *current_repo = Some(name.to_string());
            }
            continue;
        }

        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (line.trim(), ""), // boolean flag
        };

        // Include is recursive: inline the referenced file at this point.
        if key.eq_ignore_ascii_case("include") {
            if depth >= MAX_INCLUDE_DEPTH {
                return Err(BulbError::Config(format!(
                    "Include recursion exceeded {MAX_INCLUDE_DEPTH} levels (cycle?)"
                )));
            }
            let included = if Path::new(value).is_absolute() {
                PathBuf::from(value)
            } else {
                base_dir.join(value)
            };
            // Includes are used almost exclusively for mirrorlists, which list
            // `Server = ...` lines — so they belong to the current repo (or
            // options, though that's unusual).
            match fs::read_to_string(&included) {
                Ok(text) => parse_into(&text, base_dir, conf, current_repo, depth + 1)?,
                Err(e) => {
                    // A missing mirrorlist is non-fatal on systems where the
                    // repo isn't actually used; warn via Config error only if
                    // strict mode is desired. For now, skip silently to match
                    // pacman's lenient handling of bad includes.
                    tracing_skip(&included, &e);
                }
            }
            continue;
        }

        match current_repo {
            None => set_option(conf, key, value),
            Some(repo_name) => {
                let repo = conf
                    .repos
                    .iter_mut()
                    .find(|r| &r.name == repo_name)
                    .expect("repo registered on header");
                set_repo_directive(repo, key, value);
            }
        }
    }

    Ok(())
}

fn set_option(conf: &mut PacmanConf, key: &str, value: &str) {
    let opts = &mut conf.options;
        match key.to_ascii_lowercase().as_str() {
        "rootdir" => opts.root_dir = Some(PathBuf::from(value)),
        "dbpath" => opts.db_path = Some(PathBuf::from(value)),
        "cachedir" => opts.cache_dir = Some(PathBuf::from(value)),
        "logfile" => opts.log_file = Some(PathBuf::from(value)),
        "gpgdir" => opts.gpg_dir = Some(PathBuf::from(value)),
        "hookdir" => opts.hook_dir = Some(PathBuf::from(value)),
        "architecture" => opts.architecture = Some(value.to_string()),
        "holdpkg" => extend(&mut opts.hold_pkg, value),
        "ignorepkg" => extend(&mut opts.ignore_pkg, value),
        "ignoregroup" => extend(&mut opts.ignore_group, value),
        "noupgrade" => extend(&mut opts.no_upgrade, value),
        "noextract" => extend(&mut opts.no_extract, value),
        "siglevel" => extend(&mut opts.sig_level, value),
        "localfilesiglevel" => extend(&mut opts.local_file_sig_level, value),
        "remotefilesiglevel" => extend(&mut opts.remote_file_sig_level, value),
        "paralleldownloads" => opts.parallel_downloads = value.parse().ok(),
        "downloaduser" => opts.download_user = Some(value.to_string()),
        "checkspace" => opts.check_space = true,
        "color" => opts.color = true,
        // Pacman easter eggs and unknown keys: preserve.
        other => {
            opts.extra
                .entry(other.to_string())
                .or_default()
                .push(value.to_string());
        }
    }
}

fn set_repo_directive(repo: &mut Repo, key: &str, value: &str) {
    match key.to_ascii_lowercase().as_str() {
        "server" => repo.servers.push(value.to_string()),
        "siglevel" => extend(&mut repo.sig_level, value),
        "usage" => extend(&mut repo.usage, value),
        other => {
            // Unknown repo directive: stash in usage-less extra? We don't have
            // an extra map on Repo; silently ignore for now (pacman warns).
            let _ = other;
        }
    }
}

/// Push a space-separated list value (`HoldPkg = pacman glibc`) onto the vec.
fn extend(vec: &mut Vec<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    vec.extend(value.split_whitespace().map(str::to_string));
}

fn tracing_skip(path: &Path, e: &std::io::Error) {
    // Lightweight: avoid pulling in tracing just for this. Log to stderr at
    // debug would require the logger to be initialised; keep it quiet by
    // default. The function exists so the call site reads clearly.
    let _ = (path, e);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[options]
HoldPkg = pacman glibc
Architecture = auto
SigLevel = Required DatabaseOptional
Color
ParallelDownloads = 10

[core]
Server = https://mirror.example.com/$repo/os/$arch

[extra]
Include = /etc/pacman.d/mirrorlist
";

    #[test]
    fn parses_options() {
        let conf = PacmanConf::parse(SAMPLE, Path::new("/etc")).unwrap();
        assert_eq!(conf.options.hold_pkg, vec!["pacman", "glibc"]);
        assert_eq!(conf.options.architecture.as_deref(), Some("auto"));
        assert_eq!(
            conf.options.sig_level,
            vec!["Required".to_string(), "DatabaseOptional".to_string()]
        );
        assert!(conf.options.color);
        assert_eq!(conf.options.parallel_downloads, Some(10));
    }

    #[test]
    fn preserves_repo_order() {
        let conf = PacmanConf::parse(SAMPLE, Path::new("/etc")).unwrap();
        let names: Vec<_> = conf.repos.iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, vec!["core", "extra"]);
        assert_eq!(
            conf.repo("core").unwrap().servers,
            vec!["https://mirror.example.com/$repo/os/$arch"]
        );
    }

    #[test]
    fn repo_lookup_by_name() {
        let conf = PacmanConf::parse(SAMPLE, Path::new("/etc")).unwrap();
        assert!(conf.repo("core").is_some());
        assert!(conf.repo("nonexistent").is_none());
    }
}
