# bulb

A fast, atomic package manager for Arch Linux, written in Rust.

bulb is a drop-in replacement for pacman/paru with a unified `install` command,
BLAKE3 content-addressed storage, generational upgrades with instant rollback,
delta updates via bsdiff, and an interactive TUI with fuzzy search.

## Features

| Feature | Description |
|---------|-------------|
| **zstd compression** | `.pkg.tar.zst` with optimal decompression speed and ratio |
| **BLAKE3 content store** | Content-addressed storage with hardlink deduplication (51% space savings) |
| **Generation system** | Atomic upgrades with instant rollback to any previous state |
| **Parallel pipeline** | rayon-based parallel extraction with deferred sudo |
| **Memory-mapped I/O** | mmap for zero-copy decompression |
| **SQLite WAL** | Concurrent reads, single-writer transactions |
| **Delta updates** | bsdiff binary patches for incremental upgrades (applied, not created) |
| **PGP verification** | GPG signature verification for signed packages |
| **Install scripts** | pre/post install, upgrade, remove hooks from packages |
| **System hooks** | pacman-compatible hook system (transfiletriggers) |
| **pacnew/pacsave** | Config file merge handling for upgrades and removals |
| **Sandbox builds** | bubblewrap isolation for untrusted package builds |
| **AUR support** | PKGBUILD parsing, AUR RPC search, interactive selection |
| **TUI** | ratatui + nucleo fuzzy search for interactive package selection |
| **Daemon** | bulbd — Unix socket IPC with JSON-RPC for background operations |
| **Shell completions** | bash, zsh, fish completion generation |
| **Offline mode** | Install from cache without network access |
| **Resume downloads** | HTTP Range support for interrupted transfers |
| **Auto-retry** | Exponential backoff (1s → 2s → 4s → 8s → 16s) |

## Performance

Real benchmarks against pacman on the same system:

| Operation | pacman | bulb | Speedup |
|-----------|--------|------|---------|
| Install zstd package (~2MB) | 2079 ms | 25 ms | **83x** |
| Query all installed packages | 58 ms | 1 ms | **58x** |
| Query single package | 82 ms | 1 ms | **82x** |
| vercmp (1M comparisons) | — | 354 ms | — |
| Build tiny package | — | 3 ms | — |
| List generations | — | 2 ms | — |

Content store deduplication saves **51% disk space** when the same package is installed twice.

## Installation

```bash
# From source
cargo build --release
sudo cp target/release/bulb /usr/bin/

# Generate shell completions
bulb completions bash | sudo tee /usr/share/bash-completion/completions/bulb
bulb completions zsh  | sudo tee /usr/share/zsh/site-functions/_bulb
bulb completions fish | sudo tee ~/.config/fish/completions/bulb.fish
```

## Usage

### Package Management

```bash
bulb install ./package.pkg.tar.zst          # Install local package
bulb install firefox vim git                # Install from sync repos
bulb install --needed base-devel            # Skip if already installed
bulb install --force package                # Force reinstall
bulb install --download-only package        # Download without installing

bulb remove package                         # Remove a package
bulb remove --recursive package             # Remove with dependencies
```

### Unified Install Command

bulb uses a single `install` command for everything — local files, repo packages,
and AUR packages. The command auto-detects the source:

```bash
# Local file (ends with .pkg.tar.zst)
bulb install ./hello-1.0-1-x86_64.pkg.tar.zst

# Sync repository package
bulb install firefox

# Interactive TUI selection (pass query as argument)
bulb firefox
```

### Search

```bash
bulb search firefox                         # Search sync repos + AUR
bulb search --aur firefox                   # Search AUR only
```

### Query

```bash
bulb query                                  # List all installed packages
bulb query firefox                          # Show package version
bulb query firefox --info                   # Show detailed info
bulb query firefox --list                   # List files owned by package
bulb query --owner /usr/bin/bash            # Find which package owns a file
bulb query --foreign                        # List AUR/foreign packages
bulb query --unneeded                       # List orphaned packages
bulb query --upgradable                     # List packages with updates available
bulb query --search vim                     # Search installed packages
bulb query --reasons                        # Show install reason (explicit/dependency)
```

### Sync & Update

```bash
bulb sync                                   # Download sync databases from mirrors
bulb update                                 # Update all installed packages
```

### Build

```bash
bulb init my-package                        # Create new package with Bulb.toml
bulb build ./my-package                     # Build with sandbox (bwrap)
bulb build --no-sandbox ./my-package        # Build without sandbox
```

### Generations

```bash
bulb list-generations                       # Show generation history
bulb switch 42                              # Switch to generation #42
bulb rollback                               # Rollback to previous generation
```

### Cache Management

```bash
bulb cache                                  # Show cached packages
bulb cache list                             # List cached packages
bulb cache clean                            # Keep only latest version of each package
bulb cache clean --keep 3                   # Keep 3 versions of each package
bulb cache clean-all                        # Remove ALL cached packages
bulb cache size                             # Show total cache size
```

### Migration

```bash
bulb migrate                                # Import from pacman local database
bulb migrate --pacman-local /var/lib/pacman/local  # Custom pacman DB path
```

### Daemon

```bash
bulb daemon                                 # Start bulbd daemon (Unix socket IPC)
```

### AUR

```bash
bulb parse-pkgbuild ./PKGBUILD             # Parse and display AUR PKGBUILD
```

### Completions

```bash
bulb completions bash > /usr/share/bash-completion/completions/bulb
bulb completions zsh  > /usr/share/zsh/site-functions/_bulb
bulb completions fish > ~/.config/fish/completions/bulb.fish
```

## Global Options

| Flag | Description | Default |
|------|-------------|---------|
| `-r`, `--root` | Filesystem root | `/` |
| `--db-path` | SQLite database | `/var/lib/bulb/bulb.db` |
| `--store-path` | Content store | `/var/lib/bulb/content` |
| `--sync-dir` | Sync database directory | `/var/lib/bulb/sync` |
| `--offline` | Install from cache only | `false` |
| `-y`, `--noconfirm` | Skip all confirmation prompts | `false` |

```bash
bulb --offline install firefox              # Install from cache only
bulb --root /tmp/root install pkg.tar.zst   # Install to alternate root
bulb --noconfirm remove vim                 # Remove without prompting
```

## Package Format

### .pkg.tar.zst

```
package-1.0-1-x86_64.pkg.tar.zst
├── .PKGINFO          # Package metadata
├── .BUILDINFO        # Build environment info
├── install           # Install/upgrade/remove scripts (optional)
├── usr/
│   └── bin/
│       └── program
└── etc/
    └── config.conf
```

### .PKGINFO

```
pkgname = package
pkgver = 1.0-1
arch = x86_64
pkgdesc = Package description
packager = John Doe <john@example.com>
size = 1024
depend = glibc
depend = gcc-libs
optdepend = vim: for syntax highlighting
provides = package-backend
conflicts = package-legacy
replaces = package-old
backup = etc/config.conf
```

### Build Manifest (Bulb.toml)

Bulb uses its own native build format instead of PKGBUILD:

```toml
[package]
name = "hello"
version = "1.0"
release = "1"
arch = "x86_64"
desc = "Hello world package"
packager = "bulb user"
license = ["MIT"]

depends = ["glibc"]
optdepends = ["vim: for editing"]
provides = ["hello-bin"]
conflicts = ["hello-legacy"]
replaces = ["hello-old"]
backup = ["etc/hello.conf"]

# Build steps (optional)
[build]
prepare = [
    "cd $srcdir/$pkgname-$pkgver",
    "mkdir -p build",
]
build = [
    "cd build",
    "cmake ..",
    "make",
]
package = [
    "make DESTDIR=$pkgdir install",
]
```

## Architecture

```
bulb install firefox
    │
    ├── 1. Resolve dependencies (sync DBs + AUR)
    │   ├── Parse pacman.conf → repo list
    │   ├── Load sync databases (.db files)
    │   ├── Resolve recursive deps + provides
    │   └── Check conflicts, ignore lists
    │
    ├── 2. Download (parallel with Semaphore)
    │   ├── Resume from cache (HTTP Range)
    │   ├── Try delta first → bsdiff apply
    │   ├── Fallback to full download
    │   ├── Auto-retry with exponential backoff
    │   └── PGP signature verification
    │
    ├── 3. Extract (rayon parallel)
    │   ├── Decompress zstd (mmap zero-copy)
    │   ├── Extract tar entries
    │   ├── Hash with BLAKE3
    │   ├── Content store (dedup via hardlinks)
    │   └── Set file permissions
    │
    ├── 4. Apply (single sudo)
    │   ├── flock transaction lock
    │   ├── Run pre-install scripts
    │   ├── Handle pacnew/pacsave
    │   ├── Create new generation
    │   ├── Update SQLite database
    │   ├── Run system hooks
    │   └── Run post-install scripts
    │
    └── Rollback available at any point:
        bulb rollback
```

### Module Map

| Module | Purpose |
|--------|---------|
| `core/` | Version comparison (rpmvercmp), dependency parsing, pkginfo types |
| `format/` | ALPM parsers (desc, sync DB, local DB, mtree), AUR PKGBUILD parser, Bulb.toml manifest |
| `db/` | SQLite WAL database, generation management, content store with BLAKE3 |
| `resolver.rs` | Recursive dependency resolution with virtual packages, groups, replaces |
| `download.rs` | reqwest HTTP client with resume, retry, progress, BLAKE3 verification |
| `pipeline.rs` | Parallel install pipeline with rayon, deferred sudo, file conflict detection |
| `sync.rs` | Sync database parsing (zstd + gzip compressed .db files) |
| `lock.rs` | flock-based transaction locking (prevents concurrent installs) |
| `conflict.rs` | File conflict detection between packages |
| `journal.rs` | Transaction journal for crash recovery |
| `hooks.rs` | Install scripts, pacnew/pacsave handling, system hooks |
| `pgp.rs` | GPG signature verification |
| `delta.rs` | bsdiff delta apply engine for incremental updates |
| `sandbox.rs` | bubblewrap sandbox for isolated builds |
| `aur.rs` | AUR RPC v5 API client |
| `tui/` | Interactive TUI with ratatui + nucleo fuzzy search |
| `daemon/` | bulbd daemon — Unix socket IPC, JSON-RPC, cache management |
| `util/` | Filesystem utilities (atomic rename, fsync) |
| `config/` | pacman.conf parser |
| `pkginfo.rs` | PKGBUILD-style pkginfo rendering |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Partial installation (some packages failed) |
| 2 | Error |

## Feature Flags

| Flag | Description | Default |
|------|-------------|---------|
| `archlinux` | Enable Arch Linux features: AUR, sync repos, pacman.conf, daemon, update | **enabled** |

Build without Arch-specific features for use as a standalone package library:

```bash
cargo build --no-default-features
```

## Development

```bash
# Build
cargo build
cargo build --release

# Test
cargo test --features archlinux              # 82 tests
cargo test --no-default-features             # Tests without Arch features

# Benchmarks
./benchmarks/run.sh                          # Full benchmark suite
./benchmarks/compression_bench.sh            # Compression format comparison

# Lint
cargo clippy --features archlinux
```

## License

GPL-2.0-only
