# bulb

`bulb` is a small Arch-inspired package manager for my own packages. The first implemented package format is:

```text
.pkg.tar.bz3
```

That means a normal tar archive compressed with bzip3. The archive contains `.PKGINFO` plus the files to install.

## Current commands

```bash
bulb build <source-dir>
bulb install <package.pkg.tar.bz3>
bulb remove <package>
bulb query
bulb query <package>
bulb query <package> --info
bulb query <package> --list
bulb query --owner /path/to/file
```

Global options:

```bash
bulb --root /tmp/root --db-path /tmp/bulb.db install package.pkg.tar.bz3
```

Useful for testing without touching `/`.

## Own package layout

A package is a bzip3-compressed tar archive:

```text
package.pkg.tar.bz3
├── .PKGINFO
├── usr/
├── bin/
└── ...
```

`.PKGINFO` uses Arch-like metadata:

```text
pkgname = hello
pkgver = 1.0
pkgrel = 1
pkgdesc = Hello world
arch = x86_64
packager = bulb
depend = glibc
```

## Build manifest

To build a package, create `Bulb.toml` in the source directory:

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

Then run:

```bash
bulb build .
```

The source directory contents are packaged, except `Bulb.toml` itself. `.PKGINFO` is generated automatically.

## Development status

Implemented:

- Rust CLI skeleton.
- bzip3 `.pkg.tar.bz3` package support.
- `.PKGINFO` parser/renderer.
- local package build.
- local package install.
- SQLite local DB.
- installed package query.
- installed file query.
- basic file conflict detection.
- basic package removal.

Not implemented yet:

- remote repositories.
- dependency solving.
- system upgrade.
- hooks.
- signatures.
- Btrfs rollback.
- AUR support.
- TUI.
