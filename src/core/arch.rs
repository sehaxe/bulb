//! Architecture handling.
//!
//! `pacman.conf` uses `Architecture = auto`, which resolves at runtime to the
//! machine's uname architecture (e.g. `x86_64`). CachyOS and other optimized
//! derivatives use microarchitectures like `x86_64_v3` — these are treated as
//! a separate arch namespace, not a superset, exactly like ALPM does.

use std::process::Command;

/// Resolve `Architecture = auto` to a concrete architecture string.
///
/// Mirrors pacman's behaviour: runs `uname -m`. Returns `"x86_64"` on any
/// resolution failure (safe default for the dominant target).
pub fn auto_arch() -> String {
    Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "x86_64".to_string())
}

/// Does a package's `arch` field permit installation on `host_arch`?
///
/// `any` packages install everywhere. Otherwise exact match is required,
/// matching ALPM (e.g. `x86_64_v3` packages do NOT satisfy `x86_64` repos
/// and vice-versa).
pub fn arch_matches(pkg_arch: &str, host_arch: &str) -> bool {
    pkg_arch == "any" || pkg_arch == host_arch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_runs_everywhere() {
        assert!(arch_matches("any", "x86_64"));
        assert!(arch_matches("any", "aarch64"));
    }

    #[test]
    fn exact_match_required() {
        assert!(arch_matches("x86_64", "x86_64"));
        assert!(!arch_matches("x86_64_v3", "x86_64"));
        assert!(!arch_matches("x86_64", "x86_64_v3"));
    }
}
