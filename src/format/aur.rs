use crate::error::{BulbError, Result};

#[derive(Debug, Clone, Default)]
pub struct PkgBuild {
    pub pkgname: String,
    pub pkgver: String,
    pub pkgrel: String,
    pub epoch: Option<String>,
    pub arch: Option<String>,
    pub pkgdesc: Option<String>,
    pub url: Option<String>,
    pub license: Vec<String>,
    pub depends: Vec<String>,
    pub makedepends: Vec<String>,
    pub checkdepends: Vec<String>,
    pub optdepends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub backup: Vec<String>,
    pub source: Option<Vec<String>>,
    pub sha256sums: Option<Vec<String>>,
    pub sha512sums: Option<Vec<String>>,
    pub md5sums: Option<Vec<String>>,
    pub install: Option<String>,
    pub changelog: Option<String>,
}

pub fn parse_pkgbuild(content: &str) -> Result<PkgBuild> {
    let mut pkg = PkgBuild::default();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(val) = trimmed.strip_prefix("pkgname=") {
            pkg.pkgname = parse_var_value(val);
        } else if let Some(val) = trimmed.strip_prefix("pkgver=") {
            pkg.pkgver = parse_var_value(val);
        } else if let Some(val) = trimmed.strip_prefix("pkgrel=") {
            pkg.pkgrel = parse_var_value(val);
        } else if let Some(val) = trimmed.strip_prefix("epoch=") {
            pkg.epoch = Some(parse_var_value(val));
        } else if let Some(val) = trimmed.strip_prefix("arch=") {
            pkg.arch = Some(parse_array_value(val));
        } else if let Some(val) = trimmed.strip_prefix("pkgdesc=") {
            pkg.pkgdesc = Some(parse_var_value(val));
        } else if let Some(val) = trimmed.strip_prefix("url=") {
            pkg.url = Some(parse_var_value(val));
        } else if let Some(val) = trimmed.strip_prefix("license=") {
            pkg.license = parse_array(val);
        } else if let Some(val) = trimmed.strip_prefix("depends=(") {
            pkg.depends = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("makedepends=(") {
            pkg.makedepends = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("checkdepends=(") {
            pkg.checkdepends = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("optdepends=(") {
            pkg.optdepends = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("provides=(") {
            pkg.provides = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("conflicts=(") {
            pkg.conflicts = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("replaces=(") {
            pkg.replaces = parse_dep_array(val);
        } else if let Some(val) = trimmed.strip_prefix("backup=(") {
            pkg.backup = parse_array(val);
        } else if let Some(val) = trimmed.strip_prefix("source=(") {
            pkg.source = Some(parse_array(val));
        } else if let Some(val) = trimmed.strip_prefix("sha256sums=(") {
            pkg.sha256sums = Some(parse_array(val));
        } else if let Some(val) = trimmed.strip_prefix("sha512sums=(") {
            pkg.sha512sums = Some(parse_array(val));
        } else if let Some(val) = trimmed.strip_prefix("md5sums=(") {
            pkg.md5sums = Some(parse_array(val));
        } else if let Some(val) = trimmed.strip_prefix("install=") {
            pkg.install = Some(parse_var_value(val));
        } else if let Some(val) = trimmed.strip_prefix("changelog=") {
            pkg.changelog = Some(parse_var_value(val));
        }
    }

    if pkg.pkgname.is_empty() {
        return Err(BulbError::InvalidMetadata("PKGBUILD missing pkgname".into()));
    }
    if pkg.pkgver.is_empty() {
        return Err(BulbError::InvalidMetadata("PKGBUILD missing pkgver".into()));
    }
    if pkg.pkgrel.is_empty() {
        pkg.pkgrel = "1".into();
    }

    Ok(pkg)
}

fn parse_var_value(val: &str) -> String {
    let val = val.trim();
    if (val.starts_with('"') && val.ends_with('"'))
        || (val.starts_with('\'') && val.ends_with('\''))
    {
        val[1..val.len() - 1].to_string()
    } else {
        val.to_string()
    }
}

fn parse_array_value(val: &str) -> String {
    let val = val.trim();
    if val.starts_with('(') && val.ends_with(')') {
        let inner = &val[1..val.len() - 1];
        inner
            .split_whitespace()
            .map(|s| s.trim_matches('"').trim_matches('\''))
            .filter(|s| !s.is_empty())
            .next()
            .unwrap_or("")
            .to_string()
    } else {
        parse_var_value(val)
    }
}

fn parse_array(val: &str) -> Vec<String> {
    let val = val.trim();
    let inner = if val.starts_with('(') && val.ends_with(')') {
        &val[1..val.len() - 1]
    } else if val.starts_with('(') {
        &val[1..]
    } else {
        val
    };
    let inner = inner.strip_suffix(')').unwrap_or(inner);

    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in inner.chars() {
        match in_quote {
            Some(q) if ch == q => {
                in_quote = None;
            }
            Some(_) => {
                current.push(ch);
            }
            None if ch == '\'' || ch == '"' => {
                in_quote = Some(ch);
            }
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    items.push(std::mem::take(&mut current));
                }
            }
            None => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        items.push(current);
    }

    items
}

fn parse_dep_array(val: &str) -> Vec<String> {
    parse_array(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_pkgbuild() {
        let input = r#"
pkgname=hello
pkgver=1.0
pkgrel=1
arch=(x86_64)
"#;
        let pkg = parse_pkgbuild(input).unwrap();
        assert_eq!(pkg.pkgname, "hello");
        assert_eq!(pkg.pkgver, "1.0");
        assert_eq!(pkg.pkgrel, "1");
        assert_eq!(pkg.arch.as_deref(), Some("x86_64"));
    }

    #[test]
    fn parse_depends() {
        let input = r#"
pkgname=test
pkgver=1.0
pkgrel=1
depends=(glibc gcc-libs)
makedepends=(cmake)
"#;
        let pkg = parse_pkgbuild(input).unwrap();
        assert_eq!(pkg.depends, vec!["glibc", "gcc-libs"]);
        assert_eq!(pkg.makedepends, vec!["cmake"]);
    }

    #[test]
    fn parse_comments_ignored() {
        let input = r#"
# This is a comment
pkgname=test
# another comment
pkgver=2.0
pkgrel=1
"#;
        let pkg = parse_pkgbuild(input).unwrap();
        assert_eq!(pkg.pkgname, "test");
        assert_eq!(pkg.pkgver, "2.0");
    }

    #[test]
    fn missing_pkgname_fails() {
        let input = r#"
pkgver=1.0
pkgrel=1
"#;
        assert!(parse_pkgbuild(input).is_err());
    }

    #[test]
    fn parse_full_pkgbuild() {
        let input = r#"
pkgname=example
pkgver=2.3.1
pkgrel=2
epoch=1
arch=('x86_64' 'i686')
pkgdesc="An example package"
url="https://example.com"
license=('MIT' 'Apache-2.0')
depends=(glibc zlib)
makedepends=(cmake rust)
optdepends=('git: for version control')
provides=(example-bin)
conflicts=(example-git)
replaces=(old-example)
backup=(etc/example.conf)
source=(https://example.com/$pkgname-$pkgver.tar.gz)
sha256sums=('abc123')
install=example.install
changelog=CHANGELOG
"#;
        let pkg = parse_pkgbuild(input).unwrap();
        assert_eq!(pkg.pkgname, "example");
        assert_eq!(pkg.pkgver, "2.3.1");
        assert_eq!(pkg.pkgrel, "2");
        assert_eq!(pkg.epoch.as_deref(), Some("1"));
        assert_eq!(pkg.pkgdesc.as_deref(), Some("An example package"));
        assert_eq!(pkg.depends, vec!["glibc", "zlib"]);
        assert_eq!(pkg.makedepends, vec!["cmake", "rust"]);
        assert_eq!(pkg.optdepends, vec!["git: for version control"]);
        assert_eq!(pkg.provides, vec!["example-bin"]);
        assert_eq!(pkg.conflicts, vec!["example-git"]);
        assert_eq!(pkg.replaces, vec!["old-example"]);
        assert_eq!(pkg.backup, vec!["etc/example.conf"]);
        assert!(pkg.source.is_some());
        assert!(pkg.sha256sums.is_some());
    }
}
