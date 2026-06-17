# bulb development notes

## Naming

The project is named `bulb`, not `mypm`.

Binary name:

```text
bulb
```

Own package extension:

```text
.pkg.tar.bz3
```

## Why bzip3

The first custom package format uses bzip3 compression instead of zstd. The current implementation uses the Rust `bzip3` crate with the bundled bzip3 library.

This is intentionally separate from Arch's `.pkg.tar.zst` format. For now, `bulb` installs only `.pkg.tar.bz3` packages.

## MVP goal

The first milestone is a local package manager that can:

1. build a `.pkg.tar.bz3` package from a directory,
2. install it into a target root,
3. record installed files in SQLite,
4. query installed packages,
5. remove installed packages,
6. detect simple file conflicts.

## Current architecture

```text
src/
├── main.rs       # CLI entry point
├── commands.rs   # command dispatch and install/remove/query logic
├── archive.rs    # bzip3 tar archive build/extract helpers
├── db.rs         # SQLite local DB
├── pkginfo.rs    # .PKGINFO parser/renderer
├── package.rs    # package metadata namespace
└── error.rs      # shared error type
```

## Current behavior

### Build

`bulb build <dir>` expects:

```text
<dir>/Bulb.toml
<dir>/files/to/package/...
```

It generates `.PKGINFO`, creates a tar archive, compresses it with bzip3, and writes `.pkg.tar.bz3`.

### Install

`bulb install <package.pkg.tar.bz3>`:

1. opens the package,
2. reads `.PKGINFO`,
3. lists archive entries,
4. checks file conflicts against the SQLite DB,
5. extracts files into `--root`,
6. records package metadata and files,
7. removes extracted files if DB commit fails.

### Remove

`bulb remove <package>`:

1. deletes package file records from SQLite,
2. deletes package metadata,
3. removes installed files from `--root`.

Directory cleanup is intentionally conservative: empty directories are removed opportunistically, but non-empty directories are left alone.

## Next development steps

1. Add a real transaction journal.
2. Add package signature verification.
3. Add remote repo sync DB support.
4. Add dependency resolver.
5. Add Btrfs snapshot hooks.
6. Add `bulb.conf`.
7. Add tests for rollback behavior.
8. Add shell completions.
