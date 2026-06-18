use std::sync::Arc;

use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Nucleo};
use crate::core::pkginfo::PackageInfo;

pub struct FuzzySearch {
    nucleo: Nucleo<usize>,
}

impl FuzzySearch {
    pub fn new(packages: &[PackageInfo]) -> Self {
        let config = Config::DEFAULT.match_paths();
        let notify = Arc::new(|| {});
        let nucleo = Nucleo::new(config, notify, Some(1), 1);
        let injector = nucleo.injector();

        for (i, pkg) in packages.iter().enumerate() {
            let name = pkg.name.clone();
            injector.push(i, move |_idx, cols| {
                cols[0] = name.as_str().into();
            });
        }

        Self { nucleo }
    }

    pub fn search(&mut self, query: &str) -> Vec<usize> {
        self.nucleo
            .pattern
            .reparse(0, query, CaseMatching::Smart, Normalization::Smart, false);
        self.nucleo.tick(5);

        let snap = self.nucleo.snapshot();
        let mut results: Vec<usize> = snap
            .matched_items(..)
            .map(|item| *item.data)
            .collect();

        results.sort();
        results
    }

    pub fn update_packages(&mut self, packages: &[PackageInfo]) {
        self.nucleo.restart(true);
        let injector = self.nucleo.injector();
        for (i, pkg) in packages.iter().enumerate() {
            let name = pkg.name.clone();
            injector.push(i, move |_idx, cols| {
                cols[0] = name.as_str().into();
            });
        }
    }
}
