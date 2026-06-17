# bulb

A fast Arch Linux package manager written in Rust.

## Features

- **bzip3** (`.pkg.tar.bz3`) and **zstd** (`.pkg.tar.zst`) package support
- **BLAKE3 content store** with hardlink deduplication (51% space savings)
- **Generation system** — atomic upgrades with instant rollback
- **Parallel pipeline** — download → verify → stage → single sudo apply
- **Memory-mapped I/O** — zero-copy decompression
- **SQLite WAL** — concurrent reads, no locking
- **Sandbox builds** — bwrap isolation for untrusted packages
- **AUR PKGBUILD parser** — parse and convert AUR packages
- **Delta updates** — bsdiff binary patches for incremental upgrades

## Performance

| Operation | pacman | bulb | Speedup |
|-----------|--------|------|---------|
| Install zstd (1.9MB) | 1856ms | 25ms | **74x** |
| Install bz3 (2.0MB) | 2334ms | 605ms | **3.9x** |
| Query all packages | 58ms | 1ms | **58x** |
| Query single package | 74ms | 1ms | **74x** |
| vercmp (1M comparisons) | — | 344ms | — |

## Commands

```bash
# Package management
bulb build <source-dir>                    # Build .pkg.tar.bz3 from Bulb.toml
bulb build-sandbox <source-dir>            # Build inside bwrap sandbox
bulb install <package>                     # Install local package
bulb install-batch <pkg1> <pkg2> ...       # Install multiple packages in parallel
bulb install-package <name>                # Install from sync repos
bulb remove <package>                      # Remove installed package

# AUR
bulb parse-pkgbuild <PKGBUILD>            # Parse and display AUR PKGBUILD

# Query
bulb query                                 # List all installed packages
bulb query <package>                       # Show package info
bulb query <package> --info                # Detailed info
bulb query <package> --list                # List files
bulb query --owner /path/to/file           # Find package owning a file

# Sync
bulb sync                                  # Download sync databases from mirrors

# Generations
bulb list-generations                      # Show generation history
bulb switch <generation>                   # Switch to a generation
bulb rollback                              # Rollback to previous generation

# Migration
bulb migrate                               # Import pacman local database

# Benchmarks
bulb bench-decompress <pkg> -o <out>       # Pure decompression benchmark
bulb bench-sync-parse <db>                 # Sync DB parsing benchmark
bulb bench-vercmp                          # Version comparison benchmark
```

## Global Options

```bash
bulb --root /tmp/root --db-path /tmp/bulb.db --store-path /tmp/store install pkg.tar.zst
```

- `--root` — filesystem root (default: `/`)
- `--db-path` — SQLite database path (default: `/var/lib/bulb/bulb.db`)
- `--store-path` — content store path (default: `/var/lib/bulb/content`)
- `--sync-dir` — sync database directory (default: `/var/lib/bulb/sync`)

## Package Format

### bzip3 (.pkg.tar.bz3)

```text
package.pkg.tar.bz3
├── .PKGINFO
├── usr/
│   └── bin/
└── ...
```

### .PKGINFO

```text
pkgname = hello
pkgver = 1.0
pkgrel = 1
pkgdesc = Hello world
arch = x86_64
packager = bulb
depend = glibc
```

### Build Manifest (Bulb.toml)

```toml
[package]
name = "hello"
version = "1.0"
release = "1"
arch = "x86_64"
desc = "Hello world"
packager = "bulb"
depends = []
optdepends = []
provides = []
conflicts = []
replaces = []
backup = []
```

## Architecture

```
Package File → Decompress → Extract → BLAKE3 Hash → Content Store → Hardlink → /usr/bin/xxx
                                                                        ↓
                                                              SQLite (generations)
                                                                        ↓
                                                              /etc → /etc.old (rollback)
```

### Key Components

| Module | Description |
|--------|-------------|
| `core/` | Version comparison (rpmvercmp), dependencies, pkginfo |
| `format/` | ALPM format parsers (desc, sync DB, local DB, mtree), AUR PKGBUILD parser |
| `db/` | SQLite WAL, generations, content store, transactions |
| `download.rs` | reqwest HTTP/2 with BLAKE3 verification |
| `sync.rs` | Sync database parsing (zstd + gzip) |
| `resolver.rs` | Recursive dependency resolution |
| `pipeline.rs` | Parallel install pipeline with deferred sudo |
| `sandbox.rs` | bwrap sandbox for isolated builds |
| `delta.rs` | bsdiff binary delta patches for incremental updates |

## Benchmarks

Run the benchmark suite:

```bash
./benchmarks/run.sh
```

Results are saved to `benchmarks/results/` with timestamps.

## Development Status

### Completed

- Phase 0: Core abstractions (version, dependency, pkginfo, pacman.conf parser)
- Phase 1: ALPM read compatibility (desc, sync DB, local DB, mtree, pkgfile)
- Phase 2: Content store with BLAKE3 dedup, generation rollback, transactions
- Phase 3: Download pipeline, sync repos, dependency resolver, PGP stub
- Phase 4 (partial): bz3 parallel decompression, benchmarks, parallel pipeline, sandbox builds, AUR parser, delta updates

### Planned

- Phase 5: TUI (ratatui + nucleo fuzzy search)
- Phase 6: bulbd daemon

## License

MIT OR Apache-2.0
