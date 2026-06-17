use std::collections::{HashMap, HashSet};

use crate::core::dependency::Depend;
use crate::core::version::Version;
use crate::error::{BulbError, Result};

#[derive(Debug, Clone)]
pub struct PackageVersion {
    pub version: Version,
    pub depends: Vec<Depend>,
    pub provides: Vec<String>,
    pub filename: String,
    pub repo: String,
}

#[derive(Debug)]
pub struct Resolver {
    packages: HashMap<String, Vec<PackageVersion>>,
    installed: HashMap<String, Version>,
    pub hold: HashSet<String>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            installed: HashMap::new(),
            hold: HashSet::new(),
        }
    }

    pub fn add_package(&mut self, name: &str, version: PackageVersion) {
        self.packages
            .entry(name.to_string())
            .or_default()
            .push(version);
    }

    pub fn add_installed(&mut self, name: &str, version: Version) {
        self.installed.insert(name.to_string(), version);
    }

    pub fn set_hold(&mut self, packages: HashSet<String>) {
        self.hold = packages;
    }

    pub fn resolve(&self, targets: &[String]) -> Result<Vec<ResolvedPackage>> {
        let mut resolved = Vec::new();
        let mut visited = HashSet::new();

        for target in targets {
            self.resolve_package(target, &mut resolved, &mut visited)?;
        }

        Ok(resolved)
    }

    fn resolve_package(
        &self,
        name: &str,
        resolved: &mut Vec<ResolvedPackage>,
        visited: &mut HashSet<String>,
    ) -> Result<()> {
        if !visited.insert(name.to_string()) {
            return Ok(());
        }

        let versions = self.packages.get(name)
            .ok_or_else(|| BulbError::Resolver(format!("package not found: {name}")))?;

        let best = versions.iter().max_by_key(|v| &v.version)
            .ok_or_else(|| BulbError::Resolver(format!("no versions available: {name}")))?;

        for dep in &best.depends {
            let dep_name = dep.name.split('<').next()
                .and_then(|s| s.split('>').next())
                .and_then(|s| s.split('=').next())
                .unwrap_or(&dep.name);

            if !self.installed.contains_key(dep_name) {
                self.resolve_package(dep_name, resolved, visited)?;
            }
        }

        resolved.push(ResolvedPackage {
            name: name.to_string(),
            version: best.version.clone(),
            filename: best.filename.clone(),
            repo: best.repo.clone(),
            deps: best.depends.clone(),
        });

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
            filename: "libc-2.34-1.pkg.tar.zst".into(),
            repo: "core".into(),
        });

        resolver.add_package("bash", PackageVersion {
            version: Version::parse("5.2.021").unwrap(),
            depends: vec![Depend { name: "libc".into(), constraint: Default::default(), reason: None }],
            provides: vec![],
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
}
