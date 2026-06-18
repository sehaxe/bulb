//! Conversions from `desc`/`.PKGINFO` into the unified [`PackageInfo`].
//!
//! Both on-disk formats carry the same conceptual fields but with different
//! key spellings. Centralising the mapping here keeps sync/local DB parsers
//! tiny and guarantees the two paths agree on field semantics.

use crate::core::dependency::{Depend, Provide};
use crate::core::pkginfo::{PackageInfo, PackageSource};

#[cfg(feature = "archlinux")]
use super::desc::Desc;
use super::pkginfo::PkgInfo;

#[cfg(feature = "archlinux")]
pub fn package_info_from_desc(desc: &Desc, repo: Option<&str>) -> PackageInfo {
    let depends = to_depends(desc.get_vec("depends"));
    let optdepends = to_depends(desc.get_vec("optdepends"));
    let makedepends = to_depends(desc.get_vec("makedepends"));
    let checkdepends = to_depends(desc.get_vec("checkdepends"));
    let conflicts = to_depends(desc.get_vec("conflicts"));
    let replaces = to_depends(desc.get_vec("replaces"));
    let provides = to_provides(desc.get_vec("provides"));

    let source = match repo {
        Some(name) => PackageSource::Alpm {
            repo: name.to_string(),
            filename: desc.get("filename").map(str::to_string),
            csize: desc.get("csize").and_then(|s| s.parse().ok()),
            sha256: desc.get("sha256sum").map(str::to_string),
            pgpsig: desc.get("pgpsig").map(str::to_string),
        },
        None => PackageSource::Local,
    };

    PackageInfo {
        name: desc.get("name").unwrap_or_default().to_string(),
        base: desc.get("base").map(str::to_string),
        version: desc.get("version").unwrap_or_default().to_string(),
        arch: desc.get("arch").unwrap_or_default().to_string(),
        description: desc.get("desc").map(str::to_string),
        url: desc.get("url").map(str::to_string),
        packager: desc.get("packager").map(str::to_string),
        builddate: desc.get("builddate").and_then(|s| s.parse().ok()),
        installdate: desc.get("installdate").and_then(|s| s.parse().ok()),
        license: desc.get_vec("license").to_vec(),
        groups: desc.get_vec("groups").to_vec(),
        depends,
        optdepends,
        makedepends,
        checkdepends,
        provides,
        conflicts,
        replaces,
        backup: desc.get_vec("backup").to_vec(),
        // Local `desc` uses %SIZE%; sync uses %ISIZE%. Prefer isize then size.
        size: desc
            .get("isize")
            .and_then(|s| s.parse().ok())
            .or_else(|| desc.get("size").and_then(|s| s.parse().ok())),
        source,
    }
}

/// Build a [`PackageInfo`] from a `.PKGINFO` embedded in a `.pkg.tar.zst`.
pub fn package_info_from_pkginfo(p: &PkgInfo) -> PackageInfo {
    PackageInfo {
        name: p.get("pkgname").unwrap_or_default().to_string(),
        base: p.get("pkgbase").map(str::to_string),
        version: p.get("pkgver").unwrap_or_default().to_string(),
        arch: p.get("arch").unwrap_or_default().to_string(),
        description: p.get("pkgdesc").map(str::to_string),
        url: p.get("url").map(str::to_string),
        packager: p.get("packager").map(str::to_string),
        builddate: p.get("builddate").and_then(|s| s.parse().ok()),
        installdate: None,
        license: p.get_vec("license").to_vec(),
        groups: p.get_vec("groups").to_vec(),
        depends: to_depends(p.get_vec("depend")),
        optdepends: to_depends(p.get_vec("optdepend")),
        makedepends: to_depends(p.get_vec("makedepend")),
        checkdepends: to_depends(p.get_vec("checkdepend")),
        provides: to_provides(p.get_vec("provides")),
        conflicts: to_depends(p.get_vec("conflict")),
        replaces: to_depends(p.get_vec("replaces")),
        backup: p.get_vec("backup").to_vec(),
        size: p.get("size").and_then(|s| s.parse().ok()),
        source: PackageSource::Local,
    }
}

fn to_depends(values: &[String]) -> Vec<Depend> {
    values.iter().map(|v| Depend::parse(v)).collect()
}

fn to_provides(values: &[String]) -> Vec<Provide> {
    values.iter().map(|v| Provide::parse(v)).collect()
}

#[cfg(all(test, feature = "archlinux"))]
mod tests {
    use super::*;

    #[test]
    fn desc_roundtrip() {
        let desc = Desc::parse(
            "\
%NAME%
acl

%VERSION%
2.3.2-2

%ARCH%
x86_64

%DEPENDS%
glibc>=2.38
attr

%PROVIDES%
libacl.so=1-64
",
        );
        let info = package_info_from_desc(&desc, Some("core"));
        assert_eq!(info.name, "acl");
        assert_eq!(info.version, "2.3.2-2");
        assert_eq!(info.depends.len(), 2);
        assert_eq!(info.depends[0].name, "glibc");
        assert_eq!(info.provides[0].name, "libacl.so");
        match info.source {
            PackageSource::Alpm { repo, .. } => assert_eq!(repo, "core"),
            _ => panic!("expected Alpm source"),
        }
    }
}
