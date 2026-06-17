//! Format-agnostic core: version comparison, dependency model, unified
//! package metadata, and architecture handling. These types are shared by
//! every package backend so the resolver, DB, and UI never need to know
//! whether a package came from an ALPM repo or bulb's native format.

pub mod arch;
pub mod dependency;
pub mod pkginfo;
pub mod version;

pub use dependency::{Depend, Provide};
pub use pkginfo::{PackageInfo, PackageSource};
pub use version::{Constraint, Version};
