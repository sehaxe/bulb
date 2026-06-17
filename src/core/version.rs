//! Pacman/ALPM version comparison — a faithful port of ALPM's `alpm_pkg_vercmp`,
//! which is itself RPM's `rpmvercmp` adapted for Arch's needs.
//!
//! This is the single most important compatibility contract for a pacman
//! drop-in: if two versions compare differently than `pacman -Q` would order
//! them, dependency resolution breaks. The test suite at the bottom encodes
//! the 17 empirical cases verified against the real `vercmp` binary on this
//! machine and is the contract we must never regress.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::error::{BulbError, Result};

/// A parsed package version: `epoch:pkgver` with an optional `-pkgrel` suffix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub epoch: u64,
    pub pkgver: String,
    pub pkgrel: Option<String>,
}

impl Version {
    pub fn parse(input: &str) -> Result<Self> {
        if input.is_empty() {
            return Err(BulbError::InvalidVersion("empty version string".into()));
        }

        let (epoch, rest) = match input.split_once(':') {
            Some((epoch_str, rest)) => {
                let epoch: u64 = epoch_str
                    .parse()
                    .map_err(|_| BulbError::InvalidVersion(format!("bad epoch in {input:?}")))?;
                (epoch, rest)
            }
            None => (0, input),
        };

        let (pkgver, pkgrel) = match rest.split_once('-') {
            Some((pkgver, pkgrel)) if !pkgrel.is_empty() => {
                (pkgver.to_string(), Some(pkgrel.to_string()))
            }
            _ => (rest.to_string(), None),
        };

        if pkgver.is_empty() {
            return Err(BulbError::InvalidVersion(format!("empty pkgver in {input:?}")));
        }

        Ok(Version {
            epoch,
            pkgver,
            pkgrel,
        })
    }

    pub fn cmp_alpm(&self, other: &Self) -> Ordering {
        match self.epoch.cmp(&other.epoch) {
            Ordering::Equal => {}
            non_equal => return non_equal,
        }

        match rpmvercmp(&self.pkgver, &other.pkgver) {
            Ordering::Equal => {}
            non_equal => return non_equal,
        }

        match (&self.pkgrel, &other.pkgrel) {
            (Some(a), Some(b)) => rpmvercmp(a, b),
            _ => Ordering::Equal,
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp_alpm(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_alpm(other)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.epoch != 0 {
            write!(f, "{}:{}", self.epoch, self.pkgver)?;
        } else {
            f.write_str(&self.pkgver)?;
        }
        if let Some(pkgrel) = &self.pkgrel {
            write!(f, "-{pkgrel}")?;
        }
        Ok(())
    }
}

/// Dependency version constraint. Mirrors `alpm_depmod_t`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Constraint {
    Any,
    Eq(Version),
    Ge(Version),
    Gt(Version),
    Le(Version),
    Lt(Version),
}

impl Default for Constraint {
    fn default() -> Self {
        Constraint::Any
    }
}

impl Constraint {
    pub fn matches(&self, candidate: &Version) -> bool {
        match self {
            Constraint::Any => true,
            Constraint::Eq(v) => candidate.cmp_alpm(v) == Ordering::Equal,
            Constraint::Ge(v) => candidate.cmp_alpm(v) != Ordering::Less,
            Constraint::Gt(v) => candidate.cmp_alpm(v) == Ordering::Greater,
            Constraint::Le(v) => candidate.cmp_alpm(v) != Ordering::Greater,
            Constraint::Lt(v) => candidate.cmp_alpm(v) == Ordering::Less,
        }
    }

    /// `true` when this is `Constraint::Any`. Used by serde skip helpers.
    pub fn is_any(&self) -> bool {
        matches!(self, Constraint::Any)
    }
}

/// Split a constraint-bearing dependency token like `glibc>=2.38` into the
/// `(name, constraint)` pair understood by ALPM. Operators have no surrounding
/// whitespace in the on-disk format.
pub fn split_versioned(token: &str) -> (String, Constraint) {
    // Order matters: check two-char operators first so `>=` is not mistaken
    // for `>`.
    for (op, kind) in [
        (">=", Op::Ge),
        ("<=", Op::Le),
        ("=", Op::Eq),
        (">", Op::Gt),
        ("<", Op::Lt),
    ] {
        if let Some(idx) = token.find(op) {
            let name = token[..idx].to_string();
            let ver_str = &token[idx + op.len()..];
            let constraint = match Version::parse(ver_str) {
                Ok(v) => match kind {
                    Op::Eq => Constraint::Eq(v),
                    Op::Ge => Constraint::Ge(v),
                    Op::Gt => Constraint::Gt(v),
                    Op::Le => Constraint::Le(v),
                    Op::Lt => Constraint::Lt(v),
                },
                // A malformed version inside a constraint degrades to Any
                // rather than failing the whole parse — same as ALPM, which
                // would just never match.
                Err(_) => Constraint::Any,
            };
            return (name, constraint);
        }
    }
    (token.to_string(), Constraint::Any)
}

#[derive(Clone, Copy)]
enum Op {
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
}

// ---------------------------------------------------------------------------
// rpmvercmp — the core algorithm. Faithful port of lib/libalpm/version.c.
// ---------------------------------------------------------------------------

/// Compare two version strings (no epoch/pkgrel handling here) per rpmvercmp.
///
/// Returns `Less`, `Equal`, or `Greater`. The tricky rules:
///
/// - Separators are any non-alphanumeric byte, **except** `~` and `+`, which
///   are handled specially.
/// - `~` (tilde) sorts *before* everything, including end-of-string: `1~ < 1`.
/// - `+` sorts *after* everything: `1.0+ > 1.0`.
/// - A token is a maximal run of digits, or a maximal run of letters. When
///   one side starts a numeric token and the other an alpha token, the
///   numeric side wins.
/// - Numeric tokens compare with leading zeros stripped, then by length
///   (more digits = greater), then by strcmp.
/// - When all segments tie, the side with remaining characters wins.
fn rpmvercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let a = a.as_bytes();
    let b = b.as_bytes();
    rpmvercmp_bytes(a, b)
}

/// `is_segment_char`: a byte that is part of a token OR a tilde/plus marker.
/// The separator-skip loop stops at these.
fn is_segment_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'~' || c == b'+'
}

fn rpmvercmp_bytes(a: &[u8], b: &[u8]) -> Ordering {
    let (mut i, mut j) = (0usize, 0usize);

    while i < a.len() || j < b.len() {
        // Skip separators (non-segment chars).
        while i < a.len() && !is_segment_char(a[i]) {
            i += 1;
        }
        while j < b.len() && !is_segment_char(b[j]) {
            j += 1;
        }

        // Tilde: sorts before everything (including end-of-string).
        let a_til = i < a.len() && a[i] == b'~';
        let b_til = j < b.len() && b[j] == b'~';
        if a_til || b_til {
            if !a_til {
                return Ordering::Greater;
            }
            if !b_til {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }

        // Plus: sorts after everything.
        let a_plus = i < a.len() && a[i] == b'+';
        let b_plus = j < b.len() && b[j] == b'+';
        if a_plus || b_plus {
            if !a_plus {
                return Ordering::Less;
            }
            if !b_plus {
                return Ordering::Greater;
            }
            i += 1;
            j += 1;
            continue;
        }

        // If either side is exhausted, we are done comparing segments.
        if !(i < a.len() && j < b.len()) {
            break;
        }

        // Grab tokens. Token class follows side `a`'s first char. We advance
        // each side only while it matches that class, so a side starting with
        // the other class yields an empty token.
        let a_tok_start = i;
        let b_tok_start = j;
        let is_digit = a[i].is_ascii_digit();
        if is_digit {
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
        } else {
            while i < a.len() && a[i].is_ascii_alphabetic() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_alphabetic() {
                j += 1;
            }
        }
        let a_tok = &a[a_tok_start..i];
        let b_tok = &b[b_tok_start..j];

        // If b's token is empty (started with the other class): numeric a
        // wins, alpha a loses. Symmetric with the C reference.
        if b_tok.is_empty() {
            return if is_digit {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }

        let ord = if is_digit {
            cmp_numeric_bytes(a_tok, b_tok)
        } else {
            a_tok.cmp(b_tok)
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    // All segments tied: the side that still has characters left wins.
    match (i < a.len(), j < b.len()) {
        (false, false) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        // Both still have bytes — only separators/tildes/pluses remain, which
        // the loop would have resolved, so this branch is unreachable in
        // practice. Treat as equal defensively.
        (true, true) => Ordering::Equal,
    }
}

/// Compare numeric token bytes per rpmvercmp: strip leading zeros, then
/// longer wins, then strcmp.
fn cmp_numeric_bytes(a: &[u8], b: &[u8]) -> Ordering {
    let a = strip_leading_zeros(a);
    let b = strip_leading_zeros(b);
    match a.len().cmp(&b.len()) {
        Ordering::Equal => a.cmp(b),
        non_equal => non_equal,
    }
}

fn strip_leading_zeros(s: &[u8]) -> &[u8] {
    let start = s.iter().take_while(|&&c| c == b'0').count();
    // Keep at least one byte so "000" → "0".
    &s[start.min(s.len().saturating_sub(1))..]
}

// ===========================================================================
// Tests — the vercmp contract. These are the 17 empirical cases verified
// against the real `/usr/bin/vercmp` on this machine. They MUST stay green.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Ordering {
        Version::parse(a).unwrap().cmp_alpm(&Version::parse(b).unwrap())
    }

    fn sign(o: Ordering) -> i32 {
        match o {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    // Each case: (a, b, expected sign). `vercmp` returns -1/0/1.
    #[test]
    fn vercmp_contract() {
        let cases: &[(&str, &str, i32)] = &[
            ("1.0", "1.0.1", -1),         // longer version wins
            ("1.0a", "1.0b", -1),         // alpha strcmp
            ("1.0a", "1.0.1", -1),        // numeric beats alpha
            ("1.0.1", "1.0a", 1),         // reverse of above
            ("1", "1.0", -1),             // extra token wins
            ("1_0", "1.0", 0),            // `_` and `.` both separators
            ("1.0_alpha", "1.0.alpha", 0), // separators equivalent
            ("1.0+", "1.0", 1),           // `+` sep + empty token => longer wins
            ("1~", "1", -1),              // tilde sorts before nothing
            ("r100", "r99", 1),           // within `r..`, numeric 100>99
            ("2:1.0", "1:9.9", 1),        // epoch dominates
            ("1:0", "0", 1),              // epoch 1 > epoch 0
            ("010", "10", 0),             // leading zeros ignored
            ("1.0-1", "1.0", 0),          // pkgrel ignored when versions tie & one missing
            ("1-1", "1-2", -1),           // pkgrel numeric compare
            ("1.0_rc1", "1.0", 1),        // `_rc1` => extra token => greater
            ("1.0-1", "1.0-1", 0),        // identity
        ];

        for (a, b, want) in cases {
            assert_eq!(
                sign(cmp(a, b)),
                *want,
                "vercmp({a:?}, {b:?}): expected sign {want}, got {}",
                sign(cmp(a, b))
            );
        }
    }

    #[test]
    fn epoch_is_extracted() {
        let v = Version::parse("2:1.0-3").unwrap();
        assert_eq!(v.epoch, 2);
        assert_eq!(v.pkgver, "1.0");
        assert_eq!(v.pkgrel.as_deref(), Some("3"));
    }

    #[test]
    fn no_epoch_no_pkgrel() {
        let v = Version::parse("1.0").unwrap();
        assert_eq!(v.epoch, 0);
        assert_eq!(v.pkgver, "1.0");
        assert!(v.pkgrel.is_none());
    }

    #[test]
    fn display_roundtrips_epoch() {
        let v = Version::parse("1:2.3-4").unwrap();
        assert_eq!(v.to_string(), "1:2.3-4");
    }

    #[test]
    fn constraint_matches() {
        let v = |s: &str| Version::parse(s).unwrap();
        assert!(Constraint::Ge(v("2.38")).matches(&v("2.40")));
        assert!(!Constraint::Ge(v("2.40")).matches(&v("2.38")));
        assert!(Constraint::Eq(v("1.0-1")).matches(&v("1.0-1")));
        assert!(Constraint::Any.matches(&v("anything")));
    }

    #[test]
    fn split_versioned_parses_operators() {
        let (n, c) = split_versioned("glibc>=2.38");
        assert_eq!(n, "glibc");
        assert!(matches!(c, Constraint::Ge(_)));

        let (n, c) = split_versioned("libfoo.so=1-64");
        assert_eq!(n, "libfoo.so");
        assert!(matches!(c, Constraint::Eq(_)));

        let (n, c) = split_versioned("plain");
        assert_eq!(n, "plain");
        assert!(matches!(c, Constraint::Any));
    }
}
