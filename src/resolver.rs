use std::collections::{HashMap, HashSet};

use crate::core::dependency::Depend;
use crate::core::version::Version;
use crate::error::{BulbError, Result};

#[derive(Debug, Clone)]
pub struct PackageVersion {
    pub version: Version,
    pub depends: Vec<Depend>,
    pub provides: Vec<String>,
    pub groups: Vec<String>,
    pub replaces: Vec<String>,
    pub conflicts: Vec<String>,
    pub filename: String,
    pub repo: String,
}

#[derive(Debug)]
pub struct Resolver {
    packages: HashMap<String, Vec<PackageVersion>>,
    provides: HashMap<String, String>,
    groups: HashMap<String, Vec<String>>,
    installed: HashMap<String, Version>,
    replaces: HashMap<String, Vec<String>>,
    pub hold: HashSet<String>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            provides: HashMap::new(),
            groups: HashMap::new(),
            installed: HashMap::new(),
            replaces: HashMap::new(),
            hold: HashSet::new(),
        }
    }

    pub fn add_package(&mut self, name: &str, version: PackageVersion) {
        for provide in &version.provides {
            self.provides.insert(provide.clone(), name.to_string());
        }
        for group in &version.groups {
            self.groups
                .entry(group.clone())
                .or_default()
                .push(name.to_string());
        }
        if !version.replaces.is_empty() {
            self.replaces
                .entry(name.to_string())
                .or_default()
                .extend(version.replaces.clone());
        }
        self.packages
            .entry(name.to_string())
            .or_default()
            .push(version);
    }

    pub fn get_replaced_packages(&self, name: &str) -> Vec<String> {
        self.replaces.get(name).cloned().unwrap_or_default()
    }

    pub fn add_installed(&mut self, name: &str, version: Version) {
        self.installed.insert(name.to_string(), version);
    }

    pub fn set_hold(&mut self, packages: HashSet<String>) {
        self.hold = packages;
    }

    pub fn is_installed(&self, name: &str) -> bool {
        self.installed.contains_key(name)
    }

    pub fn resolve_group(&self, group: &str) -> Result<Vec<String>> {
        self.groups
            .get(group)
            .cloned()
            .ok_or_else(|| BulbError::Resolver(format!("group not found: {group}")))
    }

    pub fn resolve(&self, targets: &[String]) -> Result<Vec<ResolvedPackage>> {
        let mut resolved = Vec::new();
        let mut visited = HashSet::new();
        let mut resolved_names = HashSet::new();

        for target in targets {
            if let Some(group_packages) = self.groups.get(target) {
                for pkg_name in group_packages {
                    self.resolve_package(pkg_name, &mut resolved, &mut visited, &mut resolved_names)?;
                }
            } else {
                self.resolve_package(target, &mut resolved, &mut visited, &mut resolved_names)?;
            }
        }

        Ok(resolved)
    }

    fn resolve_package(
        &self,
        name: &str,
        resolved: &mut Vec<ResolvedPackage>,
        visited: &mut HashSet<String>,
        resolved_names: &mut HashSet<String>,
    ) -> Result<()> {
        if !visited.insert(name.to_string()) {
            return Ok(());
        }

        let real_name = self.provides.get(name).map(|s| s.as_str()).unwrap_or(name);

        if self.installed.contains_key(real_name) {
            return Ok(());
        }

        let versions = self.packages.get(real_name)
            .ok_or_else(|| BulbError::Resolver(format!("package not found: {name}")))?;

        let best = versions.iter().max_by_key(|v| &v.version)
            .ok_or_else(|| BulbError::Resolver(format!("no versions available: {name}")))?;

        for dep in &best.depends {
            let dep_name = dep.name.split('<').next()
                .and_then(|s| s.split('>').next())
                .and_then(|s| s.split('=').next())
                .unwrap_or(&dep.name);

            if !self.installed.contains_key(dep_name) && !resolved_names.contains(dep_name) {
                self.resolve_package(dep_name, resolved, visited, resolved_names)?;
            }
        }

        if !resolved_names.contains(real_name) {
            resolved_names.insert(real_name.to_string());
            resolved.push(ResolvedPackage {
                name: real_name.to_string(),
                version: best.version.clone(),
                filename: best.filename.clone(),
                repo: best.repo.clone(),
                deps: best.depends.clone(),
                groups: best.groups.clone(),
            });
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: Version,
    pub filename: String,
    pub repo: String,
    pub deps: Vec<Depend>,
    pub groups: Vec<String>,
}

impl ResolvedPackage {
    pub fn url(&self, mirror: &str) -> String {
        format!("{}/{}/{}", mirror, self.repo, self.filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_simple_dependency() {
        let mut resolver = Resolver::new();

        resolver.add_package("libc", PackageVersion {
            version: Version::parse("2.34").unwrap(),
            depends: vec![],
            provides: vec![],
            groups: vec![],
            replaces: vec![],
            conflicts: vec![],
            filename: "libc-2.34-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        resolver.add_package("bash", PackageVersion {
            version: Version::parse("5.2.021").unwrap(),
            depends: vec![Depend { name: "libc".into(), constraint: Default::default(), reason: None }],
            provides: vec![],
            groups: vec![],
            replaces: vec![],
            conflicts: vec![],
            filename: "bash-5.2.021-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        let resolved = resolver.resolve(&["bash".into()]).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name, "libc");
        assert_eq!(resolved[1].name, "bash");
    }

    #[test]
    fn skips_already_installed() {
        let mut resolver = Resolver::new();
        resolver.add_installed("libc", Version::parse("2.34").unwrap());

        resolver.add_package("bash", PackageVersion {
            version: Version::parse("5.2.021").unwrap(),
            depends: vec![Depend { name: "libc".into(), constraint: Default::default(), reason: None }],
            provides: vec![],
            groups: vec![],
            replaces: vec![],
            conflicts: vec![],
            filename: "bash-5.2.021-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        let resolved = resolver.resolve(&["bash".into()]).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "bash");
    }

    #[test]
    fn fails_on_missing_package() {
        let resolver = Resolver::new();
        let result = resolver.resolve(&["nonexistent".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn resolves_provides() {
        let mut resolver = Resolver::new();

        resolver.add_package("openssl", PackageVersion {
            version: Version::parse("3.0.0").unwrap(),
            depends: vec![],
            provides: vec!["ssl".into()],
            groups: vec![],
            replaces: vec![],
            conflicts: vec![],
            filename: "openssl-3.0.0-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        resolver.add_package("app", PackageVersion {
            version: Version::parse("1.0.0").unwrap(),
            depends: vec![Depend { name: "ssl".into(), constraint: Default::default(), reason: None }],
            provides: vec![],
            groups: vec![],
            replaces: vec![],
            conflicts: vec![],
            filename: "app-1.0.0-1.pkg.tar.zst".into(),
            repo: "extra".into(),
        });

        let resolved = resolver.resolve(&["app".into()]).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name, "openssl");
        assert_eq!(resolved[1].name, "app");
    }

    #[test]
    fn resolves_group() {
        let mut resolver = Resolver::new();

        resolver.add_package("gcc", PackageVersion {
            version: Version::parse("12.0").unwrap(),
            depends: vec![],
            provides: vec![],
            groups: vec!["base-devel".into()],
            replaces: vec![],
            conflicts: vec![],
            filename: "gcc-12.0-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        resolver.add_package("make", PackageVersion {
            version: Version::parse("4.3").unwrap(),
            depends: vec![],
            provides: vec![],
            groups: vec!["base-devel".into()],
            replaces: vec![],
            conflicts: vec![],
            filename: "make-4.3-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        let resolved = resolver.resolve(&["base-devel".into()]).unwrap();
        assert_eq!(resolved.len(), 2);
        let names: Vec<&str> = resolved.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"gcc"));
        assert!(names.contains(&"make"));
    }
}
