//! Shared error type for bulb.
//!
//! Every fallible operation returns [`Result`]. The [`BulbError`] enum is the
//! single error type used across the crate so that callers get structured
//! information without losing the original source.

use std::io;
use std::path::{PathBuf, StripPrefixError};

use thiserror::Error;
use walkdir::Error as WalkDirError;

pub type Result<T> = std::result::Result<T, BulbError>;

#[derive(Debug, Error)]
pub enum BulbError {
    // ── Package / metadata ──────────────────────────────────────────
    #[error("unsupported package format: {0}")]
    UnsupportedPackageFormat(PathBuf),

    #[error("invalid package metadata: {0}")]
    InvalidMetadata(String),

    #[error("invalid version: {0}")]
    InvalidVersion(String),

    #[error("file conflict: {path} is already owned by {owner}")]
    FileConflict { path: String, owner: String },

    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error("unsafe archive path: {0}")]
    UnsafeArchivePath(String),

    // ── Generations / DB ───────────────────────────────────────────
    #[error("generation not found: {0}")]
    GenerationNotFound(i64),

    #[error("no current generation; run `bulb migrate` or install a package first")]
    NoCurrentGeneration,

    #[error("store object missing: {0}")]
    StoreObjectMissing(String),

    // ── Configuration ──────────────────────────────────────────────
    #[error("configuration error: {0}")]
    Config(String),

    #[error("migration source not found: {0}")]
    MigrationSourceNotFound(PathBuf),

    // ── Version / dependency ───────────────────────────────────────
    #[error("dependency resolution failed: {0}")]
    Resolver(String),

    #[error("unsatisfied dependency: {dep} required by {by}")]
    UnsatisfiedDep { dep: String, by: String },

    // ── Compression / archive ──────────────────────────────────────
    #[error("decompression failed: {0}")]
    Decompress(String),

    #[error("malformed tarball: {0}")]
    MalformedTarball(String),

    // ── Signature / crypto ─────────────────────────────────────────
    #[error("signature verification failed: {0}")]
    Signature(String),

    #[error("unknown PGP key: {0}")]
    UnknownKey(String),

    // ── I/O and crate conversions ──────────────────────────────────
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Bzip3(#[from] bzip3::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    WalkDir(#[from] WalkDirError),

    #[error(transparent)]
    StripPrefix(#[from] StripPrefixError),
}
