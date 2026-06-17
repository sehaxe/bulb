# bulb package format

`bulb` currently uses its own package format:

```text
.pkg.tar.bz3
```

This is a tar archive compressed with bzip3.

## Archive contents

Every package must contain `.PKGINFO` at the archive root.

Example:

```text
.pkg.tar.bz3
├── .PKGINFO
├── usr/
│   └── bin/
│       └── hello
└── usr/
    └── share/
        └── doc/
            └── hello/
                └── README
```

## .PKGINFO

`.PKGINFO` is key-value metadata:

```text
pkgname = hello
pkgver = 1.0
pkgrel = 1
pkgdesc = Hello world
arch = x86_64
url = https://example.com/hello
packager = bulb
depend = glibc
depend = bash
```

Required fields:

- `pkgname`
- `pkgver`
- `pkgrel`
- `arch`

Optional fields currently supported:

- `pkgdesc`
- `url`
- `packager`
- `size`
- `depend`
- `optdepend`
- `provides`
- `conflict`
- `replaces`
- `backup`

## Build manifest

Packages are built from a directory containing `Bulb.toml`:

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

Run:

```bash
bulb build .
```

The generated package name is:

```text
<name>-<version>-<release>-<arch>.pkg.tar.bz3
```

## Safety rules

The installer rejects archive paths that would escape the install root:

- absolute paths,
- `..` path components,
- Windows drive prefixes.

This is an early safety layer. It does not yet replace a full transaction/rollback system.
