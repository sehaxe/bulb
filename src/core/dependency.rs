//! Dependency representation shared across all package backends.
//!
//! A dependency is a name plus an optional version constraint, parsed from
//! strings like `glibc>=2.38` or `libfoo.so=1-64` (the ALPM on-disk syntax,
//! which has no whitespace around the operator).

use crate::core::version::{Constraint, Version};
use serde::{Deserialize, Serialize};

/// A single dependency: `name` plus an optional version constraint.
///
/// `desc` is the optional human-readable reason carried by `optdepends`
/// (e.g. `name: reason`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Depend {
    pub name: String,
    #[serde(default)]
    pub constraint: Constraint,
    /// Optional reason text for optdepends (`name: reason`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Depend {
    /// Parse an ALPM dependency token: `[name][op][version]` or, for
    /// optdepends, `name: reason` / `name>=ver: reason`.
    pub fn parse(token: &str) -> Self {
        // Split off an optional `: reason` suffix (optdepends) first.
        let (dep_part, reason) = match token.split_once(':') {
            Some((d, r)) => (d.trim(), Some(r.trim().to_string())),
            None => (token.trim(), None),
        };

        let (name, constraint) = crate::core::version::split_versioned(dep_part);
        Depend {
            name,
            constraint,
            reason,
        }
    }

    /// Does `candidate` (a version of a package that provides this dep's name)
    /// satisfy this dependency?
    pub fn matches(&self, candidate: &Version) -> bool {
        self.constraint.matches(candidate)
    }
}

impl std::fmt::Display for Depend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)?;
        match &self.constraint {
            Constraint::Any => {}
            Constraint::Eq(v) => write!(f, "={v}")?,
            Constraint::Ge(v) => write!(f, ">={v}")?,
            Constraint::Gt(v) => write!(f, ">{v}")?,
            Constraint::Le(v) => write!(f, "<={v}")?,
            Constraint::Lt(v) => write!(f, "<{v}")?,
        }
        if let Some(reason) = &self.reason {
            write!(f, ": {reason}")?;
        }
        Ok(())
    }
}

/// A `provides` entry. ALPM's `provides` can be versioned, e.g.
/// `libfoo.so=1-64` means "this package provides libfoo.so at version 1-64".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provide {
    pub name: String,
    #[serde(default, skip_serializing_if = "Constraint::is_any")]
    pub constraint: Constraint,
}

impl Provide {
    /// Parse a provides token. `libfoo.so=1-64` → name + Eq(1-64).
    pub fn parse(token: &str) -> Self {
        let (name, constraint) = crate::core::version::split_versioned(token);
        Provide { name, constraint }
    }
}

impl std::fmt::Display for Provide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)?;
        match &self.constraint {
            Constraint::Any => {}
            Constraint::Eq(v) => write!(f, "={v}")?,
            Constraint::Ge(v) => write!(f, ">={v}")?,
            Constraint::Gt(v) => write!(f, ">{v}")?,
            Constraint::Le(v) => write!(f, "<={v}")?,
            Constraint::Lt(v) => write!(f, "<{v}")?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain() {
        let d = Depend::parse("glibc");
        assert_eq!(d.name, "glibc");
        assert!(matches!(d.constraint, Constraint::Any));
        assert!(d.reason.is_none());
    }

    #[test]
    fn parse_versioned() {
        let d = Depend::parse("glibc>=2.38");
        assert_eq!(d.name, "glibc");
        assert!(matches!(d.constraint, Constraint::Ge(_)));
    }

    #[test]
    fn parse_so_provides() {
        let d = Depend::parse("libreadline.so=8-64");
        assert_eq!(d.name, "libreadline.so");
        assert!(matches!(d.constraint, Constraint::Eq(_)));
    }

    #[test]
    fn parse_optdepend_reason() {
        let d = Depend::parse("perl: for gprofng-display-html");
        assert_eq!(d.name, "perl");
        assert!(matches!(d.constraint, Constraint::Any));
        assert_eq!(d.reason.as_deref(), Some("for gprofng-display-html"));
    }

    #[test]
    fn parse_optdepend_versioned_reason() {
        let d = Depend::parse("foo>=1.0: needs at least 1.0");
        assert_eq!(d.name, "foo");
        assert!(matches!(d.constraint, Constraint::Ge(_)));
        assert_eq!(d.reason.as_deref(), Some("needs at least 1.0"));
    }

    #[test]
    fn provide_roundtrip() {
        let p = Provide::parse("libctf.so=0-64");
        assert_eq!(p.name, "libctf.so");
        assert!(matches!(p.constraint, Constraint::Eq(_)));
        assert!(p.to_string().contains("libctf.so"));
    }
}
