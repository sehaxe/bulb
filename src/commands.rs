use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use bulb::core::pkginfo::PackageInfo;
use bulb::db::Database;
use bulb::error::{BulbError, Result};
use bulb::format::native::package as native_pkg;

#[derive(Debug, Parser)]
#[command(name = "bulb", version, about = "A fast Arch Linux package manager")]
pub struct Cli {
    #[arg(short = 'r', long, default_value = "/")]
    pub root: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/bulb.db")]
    pub db_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/content")]
    pub store_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/sync")]
    pub sync_dir: PathBuf,

    #[arg(long, help = "Offline mode: use cached packages only")]
    pub offline: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(help = "Search query (shorthand for 'bulb search <query>')")]
    pub query: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Initialize a new package directory")]
    Init {
        #[arg(help = "Package name (default: current directory name)")]
        name: Option<String>,
        #[arg(short, long, help = "Package description")]
        desc: Option<String>,
        #[arg(short = 'v', long, default_value = "1.0", help = "Version")]
        version: String,
    },

    #[command(about = "Install a local .pkg.tar.zst package")]
    Install { package: PathBuf },

    #[command(about = "Install multiple packages in parallel (pipeline mode)")]
    InstallBatch {
        packages: Vec<PathBuf>,
        #[arg(long)]
        noconfirm: bool,
    },

    #[command(about = "Remove an installed package")]
    Remove { package: String },

    #[command(about = "Query installed packages")]
    Query {
        package: Option<String>,
        #[arg(short, long)]
        info: bool,
        #[arg(short = 'l', long)]
        list: bool,
        #[arg(short = 'o', long, value_name = "PATH")]
        owner: Option<PathBuf>,
    },

    #[command(about = "Build a local .pkg.tar.zst package from a directory containing Bulb.toml")]
    Build {
        source_dir: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, help = "Build without sandbox")]
        no_sandbox: bool,
    },

    #[command(about = "Parse and display a PKGBUILD (AUR format)")]
    ParsePkgbuild {
        path: PathBuf,
    },

    #[command(about = "Migrate from pacman local database")]
    Migrate {
        #[arg(long, default_value = "/var/lib/pacman/local")]
        pacman_local: PathBuf,
    },

    #[command(about = "List generations")]
    ListGenerations,

    #[command(about = "Switch to a specific generation")]
    Switch {
        generation: i64,
    },

    #[command(about = "Rollback to the previous generation")]
    Rollback,

    #[command(about = "Sync package databases from mirrors")]
    Sync,

    #[command(about = "Install a package from repositories")]
    InstallPackage { package: String },

    #[command(about = "Update all installed packages")]
    Update,

    #[command(about = "Interactive TUI with fuzzy search")]
    Tui,

    #[command(about = "Show package cache status")]
    Cache,
}

pub fn run(cli: Cli) -> Result<()> {
    // If no command but query provided, treat as search
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            if let Some(query) = &cli.query {
                return search_packages(query, false, cli.offline, &cli.sync_dir);
            }
            eprintln!("No command specified. Use --help for usage.");
            std::process::exit(1);
        }
    };

    match command {
        Commands::Init { name, desc, version } => {
            let name = match name {
                Some(n) => n,
                None => {
                    std::env::current_dir()
                        .map_err(|e| BulbError::Config(format!("cannot get current dir: {e}")))?
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .ok_or_else(|| BulbError::Config("cannot determine package name from current directory".into()))?
                }
            };
            init_package(&name, desc.as_deref(), &version)
        }
        Commands::Install { package } => install(&package, &cli.root, &cli.db_path, &cli.store_path),
        Commands::InstallBatch { packages, noconfirm: _ } => {
            use bulb::pipeline::InstallPlan;

            let mut plan = InstallPlan::new(
                cli.root.clone(),
                cli.db_path.clone(),
                cli.store_path.clone(),
            )?;

            for pkg in &packages {
                plan.queue(pkg.clone());
            }

            let result = plan.execute()?;

            for msg in &result.installed {
                println!("installed {msg}");
            }
            for err in &result.errors {
                eprintln!("error: {err}");
            }

            if !result.errors.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        Commands::Remove { package } => remove(&package, &cli.root, &cli.db_path),
        Commands::Query {
            package,
            info,
            list,
            owner,
        } => query(package, info, list, owner, &cli.root, &cli.db_path),
        Commands::Build { source_dir, output, no_sandbox } => {
            let manifest_path = source_dir.join("Bulb.toml");
            let manifest_text = fs::read_to_string(&manifest_path)?;
            let manifest: bulb::format::native::manifest::BuildManifest = toml::from_str(&manifest_text)?;
            let info = native_pkg::manifest_to_pkginfo(&manifest);

            let output = output
                .map(PathBuf::from)
                .unwrap_or_else(|| source_dir.join(native_pkg::package_file_name(&info)));

            if !no_sandbox && bulb::sandbox::SandboxRunner::is_available() {
                let mut config = bulb::sandbox::SandboxConfig::new(source_dir.clone(), output.clone());
                config.allow_network = false;
                let result = bulb::sandbox::SandboxRunner::run(&config)?;
                println!("built (sandbox) {}", result.display());
                return Ok(());
            }

            if !no_sandbox {
                eprintln!("warning: bwrap not found, building without sandbox");
            }

            let dir = tempfile::tempdir()?;
            let tar_path = dir.path().join("package.tar");
            {
                let tar_file = fs::File::create(&tar_path)?;
                let mut builder = tar::Builder::new(tar_file);

                let pkginfo = bulb::pkginfo::render_pkginfo(&into_old_pkginfo(&info));
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(pkginfo.as_bytes().len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append_data(&mut header, ".PKGINFO", pkginfo.as_bytes())?;

                for entry in walkdir::WalkDir::new(&source_dir).follow_links(false) {
                    let entry = entry?;
                    let path = entry.path();
                    let relative = path.strip_prefix(&source_dir)?;
                    if relative.as_os_str().is_empty() || relative == Path::new("Bulb.toml") {
                        continue;
                    }
                    builder.append_path_with_name(path, relative)?;
                }
                builder.finish()?;
            }

            let mut encoder = zstd::stream::Encoder::new(
                fs::File::create(&output)?,
                19,
            )?;
            std::io::copy(&mut fs::File::open(&tar_path)?, &mut encoder)?;
            encoder.finish()?;

            println!("built {}", output.display());
            Ok(())
        }
        Commands::ParsePkgbuild { path } => {
            let content = fs::read_to_string(&path)?;
            let pkg = bulb::format::aur::parse_pkgbuild(&content)?;
            println!("pkgname   = {}", pkg.pkgname);
            println!("pkgver    = {}", pkg.pkgver);
            println!("pkgrel    = {}", pkg.pkgrel);
            if let Some(arch) = &pkg.arch {
                println!("arch      = {arch}");
            }
            if let Some(desc) = &pkg.pkgdesc {
                println!("pkgdesc   = {desc}");
            }
            if let Some(url) = &pkg.url {
                println!("url       = {url}");
            }
            if !pkg.depends.is_empty() {
                println!("depends   = {}", pkg.depends.join(" "));
            }
            if !pkg.makedepends.is_empty() {
                println!("makedepends = {}", pkg.makedepends.join(" "));
            }
            if !pkg.provides.is_empty() {
                println!("provides  = {}", pkg.provides.join(" "));
            }
            if !pkg.conflicts.is_empty() {
                println!("conflicts = {}", pkg.conflicts.join(" "));
            }
            if !pkg.replaces.is_empty() {
                println!("replaces  = {}", pkg.replaces.join(" "));
            }
            if let Some(source) = &pkg.source {
                println!("source    = {}", source.join(" "));
            }
            if let Some(sha256sums) = &pkg.sha256sums {
                println!("sha256sums = {}", sha256sums.join(" "));
            }
            Ok(())
        }
        Commands::Migrate { pacman_local } => {
            let mut db = Database::open(&cli.db_path)?;
            let gen_id = bulb::db::migrate_from_alpm::migrate_from_alpm(&mut db, &pacman_local)?;
            println!("migrated to generation #{gen_id}");
            Ok(())
        }
        Commands::ListGenerations => {
            let db = Database::open(&cli.db_path)?;
            let gens = db.list_generations()?;
            for (id, parent, note, is_current) in &gens {
                let marker = if *is_current { " *" } else { "" };
                let parent_str = parent.map(|p| p.to_string()).unwrap_or_default();
                println!(
                    "{id:>6}  parent={parent_str:>6}  {note}{marker}"
                );
            }
            Ok(())
        }
        Commands::Switch { generation } => {
            let db = Database::open(&cli.db_path)?;
            let old_gen = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
            db.switch_generation(generation)?;
            let store = bulb::db::store::ContentStore::new(cli.store_path.clone());
            db.switch_generation_files(old_gen, generation, &cli.root, &store)?;
            println!("switched to generation #{generation}");
            Ok(())
        }
        Commands::Rollback => {
            let db = Database::open(&cli.db_path)?;
            let current = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
            if current <= 1 {
                return Err(BulbError::InvalidMetadata("no previous generation to rollback".into()));
            }
            let old_gen = current;
            db.switch_generation(current - 1)?;
            let store = bulb::db::store::ContentStore::new(cli.store_path.clone());
            db.switch_generation_files(old_gen, current - 1, &cli.root, &store)?;
            println!("rolled back to generation #{}", current - 1);
            Ok(())
        }
        Commands::Sync => {
            let conf = bulb::config::pacman_conf::PacmanConf::load(std::path::Path::new("/etc/pacman.conf"))?;
            fs::create_dir_all(&cli.sync_dir)?;

            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

            for repo in &conf.repos {
                let mirror = repo.servers.first().cloned()
                    .unwrap_or_else(|| format!("https://mirror.rackspace.com/archlinux/{}", repo.name));
                let db_url = format!("{}/{}.db", mirror.trim_end_matches('/'), repo.name);
                let dest = cli.sync_dir.join(format!("{}.db", repo.name));

                let client = reqwest::Client::builder()
                    .user_agent("bulb/0.1")
                    .build()
                    .map_err(|e| BulbError::Config(format!("http client: {e}")))?;

                let response = rt.block_on(client.get(&db_url).send())
                    .map_err(|e| BulbError::Config(format!("sync failed for {}: {e}", repo.name)))?;

                if !response.status().is_success() {
                    return Err(BulbError::Config(format!(
                        "sync failed: HTTP {} for {}",
                        response.status(), db_url
                    )));
                }

                let bytes = rt.block_on(response.bytes())
                    .map_err(|e| BulbError::Config(format!("read failed for {}: {e}", repo.name)))?;

                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&dest, &bytes)?;

                println!("synced {}", repo.name);
            }
            Ok(())
        }
        Commands::InstallPackage { package } => {
            let conf = bulb::config::pacman_conf::PacmanConf::load(std::path::Path::new("/etc/pacman.conf"))?;
            let cache_dir = cli.store_path.parent().unwrap_or(&cli.store_path).join("cache");
            fs::create_dir_all(&cache_dir)?;

            let mut found = None;
            let mut found_repo = String::new();
            for repo in &conf.repos {
                let db_path = cli.sync_dir.join(format!("{}.db", repo.name));
                if !db_path.exists() {
                    continue;
                }
                let pkgs = bulb::sync::SyncDb::parse_sync_db(&db_path)?;
                for pkg in &pkgs {
                    if pkg.name == package {
                        found = Some(pkg.clone());
                        found_repo = repo.name.clone();
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }

            let pkg = found.ok_or_else(|| BulbError::PackageNotFound(package.clone()))?;
            let filename = pkg.filename.ok_or_else(|| {
                BulbError::InvalidMetadata(format!("package {} has no filename", pkg.name))
            })?;

            let pkg_path = cache_dir.join(&filename);

            if cli.offline {
                if !pkg_path.exists() {
                    return Err(BulbError::Config(format!(
                        "offline mode: {} not in cache. Run `bulb install-package {}` with internet first.",
                        filename, package
                    )));
                }

                let sync_db_path = cli.sync_dir.join(format!("{}.db", found_repo));
                let db_age = fs::metadata(&sync_db_path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| d.as_secs() / 86400);

                if let Some(days) = db_age {
                    if days > 7 {
                        eprintln!("warning: sync database is {} days old", days);
                        eprintln!("         cached version may not be latest. Run `bulb sync` with internet to update.");
                    }
                }

                eprintln!("using cached: {}", filename);
                return install(&pkg_path, &cli.root, &cli.db_path, &cli.store_path);
            }

            if pkg_path.exists() {
                let sync_db_path = cli.sync_dir.join(format!("{}.db", found_repo));
                let db_age = fs::metadata(&sync_db_path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| d.as_secs() / 86400);

                if let Some(days) = db_age {
                    if days > 7 {
                        eprintln!("note: cached version may be outdated (sync DB is {} days old)", days);
                    }
                }

                eprintln!("using cached: {}", filename);
                return install(&pkg_path, &cli.root, &cli.db_path, &cli.store_path);
            }

            let mirror = conf.repos.iter()
                .find(|r| r.name == found_repo)
                .and_then(|r| r.servers.first())
                .cloned()
                .unwrap_or_else(|| format!("https://mirror.rackspace.com/archlinux/{}", found_repo));
            let system_arch = std::env::consts::ARCH;
            let mirror = mirror.replace("$repo", &found_repo).replace("$arch", system_arch);
            let url = format!("{}/{}", mirror.trim_end_matches('/'), filename);

            let client = bulb::download::DownloadClient::new(cache_dir, 4)?;
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

            let downloaded = rt.block_on(client.download(&url, pkg.sha256.as_deref()))?;
            drop(rt);

            install(&downloaded, &cli.root, &cli.db_path, &cli.store_path)
        }
        Commands::Update => update_all(cli.offline, &cli.sync_dir, &cli.root, &cli.db_path, &cli.store_path),
        Commands::Tui => {
            bulb::tui::run_app(cli.root, cli.db_path, cli.store_path)
        }
        Commands::Cache => {
            let cache_dir = cli.store_path.parent().unwrap_or(&cli.store_path).join("cache");
            show_cache_status(&cache_dir)
        }
    }
}

fn init_package(name: &str, desc: Option<&str>, version: &str) -> Result<()> {
    let dir = std::path::Path::new(name);

    if dir.exists() {
        return Err(BulbError::Config(format!("directory '{}' already exists", name)));
    }

    fs::create_dir_all(dir)?;
    fs::create_dir_all(dir.join("usr/bin"))?;

    let description = desc.unwrap_or("A package");

    let bulb_toml = format!(
        r#"[package]
name = "{name}"
version = "{version}"
release = "1"
arch = "x86_64"
desc = "{description}"
packager = "bulb user"
depends = []
optdepends = []
provides = []
conflicts = []
replaces = []
backup = []
"#,
        name = name,
        version = version,
        description = description,
    );

    fs::write(dir.join("Bulb.toml"), bulb_toml)?;

    println!("initialized package '{}' in ./{name}/", name);
    println!();
    println!("next steps:");
    println!("  1. cd {name}");
    println!("  2. edit Bulb.toml");
    println!("  3. add your files to usr/bin/");
    println!("  4. bulb build .");

    Ok(())
}

fn into_old_pkginfo(info: &PackageInfo) -> bulb::pkginfo::PackageInfo {
    let version_parts: Vec<&str> = info.version.split('-').collect();
    let version = version_parts.first().unwrap_or(&"0").to_string();
    let release = version_parts.get(1).unwrap_or(&"1").to_string();
    bulb::pkginfo::PackageInfo {
        name: info.name.clone(),
        version,
        release,
        arch: info.arch.clone(),
        description: info.description.clone(),
        url: info.url.clone(),
        packager: info.packager.clone(),
        size: info.size,
        license: info.license.clone(),
        depends: info.depends.iter().map(|d| d.to_string()).collect(),
        optdepends: info.optdepends.iter().map(|d| d.to_string()).collect(),
        provides: info.provides.iter().map(|p| p.to_string()).collect(),
        conflicts: info.conflicts.iter().map(|d| d.to_string()).collect(),
        replaces: info.replaces.iter().map(|d| d.to_string()).collect(),
        backup: info.backup.clone(),
    }
}

fn install(package: &Path, root: &Path, db_path: &Path, store_path: &Path) -> Result<()> {
    let file_name = package.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let mut db = Database::open(db_path)?;
    let gen_id = db.ensure_generation()?;

    let store = bulb::db::store::ContentStore::new(store_path.to_path_buf());
    store.init()?;

    let (info, extracted_files) = if file_name.ends_with(".pkg.tar.zst") {
        let file = fs::File::open(package)?;
        let buf_reader = std::io::BufReader::with_capacity(1024 * 1024, file);
        let decoder = zstd::stream::Decoder::with_buffer(buf_reader)?;
        let mut archive = tar::Archive::new(decoder);
        single_pass_extract(&mut archive, root, &store)?
    } else {
        return Err(BulbError::UnsupportedPackageFormat(package.to_path_buf()));
    };

    if let Some(owner) = db.find_file_owner(gen_id, &info.name)? {
        return Err(BulbError::FileConflict {
            path: info.name.clone(),
            owner,
        });
    }

    let new_gen = db.create_generation(&format!("install {}", info.name))?;
    db.insert_installed_package(new_gen, &info, &extracted_files, &format!("installed-{}", info.name))?;

    println!("installed {} {}", info.name, info.version);
    Ok(())
}

fn single_pass_extract<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    root: &Path,
    store: &bulb::db::store::ContentStore,
) -> Result<(PackageInfo, Vec<PathBuf>)> {
    let mut pkginfo_text = None;
    let mut files = Vec::new();
    let mut created_dirs = std::collections::HashSet::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let file_name = entry_path.file_name().and_then(|n| n.to_str());

        match file_name {
            Some(".PKGINFO") => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                pkginfo_text = Some(text);
            }
            Some(".BUILDINFO") | Some("install") | Some(".MTREE") => {}
            _ => {
                let relative = match normalize_path(&entry_path) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if relative.as_os_str().is_empty() {
                    continue;
                }

                let dest = root.join(&relative);
                match entry.header().entry_type() {
                    tar::EntryType::Directory => {
                        if created_dirs.insert(dest.clone()) {
                            let _ = fs::create_dir(&dest);
                        }
                    }
                    tar::EntryType::Regular => {
                        ensure_parent_dir(&dest, &root, &mut created_dirs)?;
                        let mut data = Vec::new();
                        entry.read_to_end(&mut data)?;
                        let hash = store.add(&data)?;
                        store.link(&hash, &dest)?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            if let Ok(mode) = entry.header().mode() {
                                let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(mode));
                            }
                        }
                    }
                    tar::EntryType::Symlink => {
                        ensure_parent_dir(&dest, &root, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let _ = fs::remove_file(&dest);
                            #[cfg(unix)]
                            std::os::unix::fs::symlink(&link_target, &dest)?;
                        }
                    }
                    tar::EntryType::Link => {
                        ensure_parent_dir(&dest, &root, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let link_dest = root.join(&link_target);
                            let _ = fs::remove_file(&dest);
                            fs::hard_link(&link_dest, &dest)?;
                        }
                    }
                    _ => continue,
                }
                files.push(relative);
            }
        }
    }

    let pkginfo_text = pkginfo_text.ok_or_else(|| {
        BulbError::InvalidMetadata("archive missing .PKGINFO".into())
    })?;
    let pkginfo = bulb::format::alpm::pkginfo::PkgInfo::parse(&pkginfo_text);
    let info = bulb::format::alpm::convert::package_info_from_pkginfo(&pkginfo);

    Ok((info, files))
}

fn ensure_parent_dir(
    path: &Path,
    root: &Path,
    created_dirs: &mut std::collections::HashSet<PathBuf>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if parent != root && !created_dirs.contains(parent) {
            let mut current = parent.to_path_buf();
            let mut stack = Vec::new();
            while current != root && !created_dirs.contains(&current) {
                stack.push(current.clone());
                match current.parent() {
                    Some(p) if p != current => current = p.to_path_buf(),
                    _ => break,
                }
            }
            for dir in stack.into_iter().rev() {
                if created_dirs.insert(dir.clone()) {
                    let _ = fs::create_dir(&dir);
                }
            }
        }
    }
    Ok(())
}

fn normalize_path(path: &Path) -> Result<PathBuf> {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(BulbError::UnsafeArchivePath(path.display().to_string()));
            }
        }
    }
    Ok(normalized)
}

fn remove(package: &str, root: &Path, db_path: &Path) -> Result<()> {
    let mut db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;

    let files = db.get_installed_files(gen_id, package)?;
    let info = db.get_installed_package(gen_id, package)?
        .ok_or_else(|| BulbError::PackageNotFound(package.into()))?;

    let new_gen = db.create_generation(&format!("remove {package}"))?;
    db.remove_package(new_gen, package)?;

    for file in files.iter().rev() {
        let path = root.join(file);
        if path.is_file() || fs::symlink_metadata(&path).is_ok() {
            fs::remove_file(&path)?;
        } else if path.is_dir() {
            let _ = fs::remove_dir(&path);
        }
    }

    println!("removed {} {}", info.name, info.version);
    Ok(())
}

fn search_packages(query: &str, aur_only: bool, offline: bool, sync_dir: &Path) -> Result<()> {
    let mut results: Vec<(String, String, String, String)> = Vec::new();

    if !aur_only {
        if let Ok(entries) = fs::read_dir(sync_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("db") {
                    if let Ok(pkgs) = bulb::sync::SyncDb::parse_sync_db(&path) {
                        let query_lower = query.to_lowercase();
                        for pkg in &pkgs {
                            if pkg.name.to_lowercase().contains(&query_lower)
                                || pkg.description.as_deref().map_or(false, |d| d.to_lowercase().contains(&query_lower))
                            {
                                let repo = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                                results.push((
                                    format!("[{repo}]"),
                                    pkg.name.clone(),
                                    pkg.version.to_string(),
                                    pkg.description.clone().unwrap_or_default(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if !aur_only && !offline {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

        match rt.block_on(bulb::aur::search_aur(query)) {
            Ok(aur_results) => {
                for pkg in aur_results {
                    results.push((
                        "[aur]".into(),
                        pkg.name,
                        pkg.version,
                        pkg.description.unwrap_or_default(),
                    ));
                }
            }
            Err(e) => {
                eprintln!("AUR search failed: {e}");
            }
        }
    } else if offline && !aur_only {
        eprintln!("offline mode: skipping AUR search");
    }

    results.sort_by(|a, b| a.1.cmp(&b.1));
    results.dedup_by(|a, b| a.1 == b.1);

    if results.is_empty() {
        println!("No results for '{query}'");
        return Ok(());
    }

    for (i, (repo, name, version, desc)) in results.iter().enumerate() {
        println!(" {} {} {} {}", i + 1, repo, name, version);
        if !desc.is_empty() {
            println!("     {desc}");
        }
    }

    println!();
    print!("[1-{}]> ", results.len());
    use std::io::Write;
    std::io::stdout().flush().unwrap_or(());

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap_or(0);
    let input = input.trim();

    if input.is_empty() {
        return Ok(());
    }

    let selection: usize = match input.parse() {
        Ok(n) if n >= 1 && n <= results.len() => n,
        _ => {
            eprintln!("Invalid selection: {input}");
            return Ok(());
        }
    };

    let (_, name, _, _) = &results[selection - 1];

    let conf = bulb::config::pacman_conf::PacmanConf::load(std::path::Path::new("/etc/pacman.conf"))
        .map_err(|e| BulbError::Config(format!("pacman.conf: {e}")))?;

    let cache_dir = sync_dir.parent().unwrap_or(sync_dir).join("cache");
    fs::create_dir_all(&cache_dir)?;

    let system_arch = std::env::consts::ARCH;

    for repo in &conf.repos {
        let db_path = sync_dir.join(format!("{}.db", repo.name));
        if !db_path.exists() {
            continue;
        }
        if let Ok(pkgs) = bulb::sync::SyncDb::parse_sync_db(&db_path) {
            for pkg in &pkgs {
                if pkg.name == *name {
                    let mirror = repo.servers.first()
                        .cloned()
                        .unwrap_or_else(|| format!("https://mirror.rackspace.com/archlinux/{}", repo.name));
                    let mirror = mirror.replace("$repo", &repo.name).replace("$arch", system_arch);
                    let filename = pkg.filename.as_deref().unwrap_or("");
                    let url = format!("{}/{}", mirror.trim_end_matches('/'), filename);

                    let client = bulb::download::DownloadClient::new(cache_dir, 4)?;
                    let rt = tokio::runtime::Runtime::new()
                        .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

                    let pkg_path = rt.block_on(client.download(&url, pkg.sha256.as_deref()))?;
                    drop(rt);

                    let _pkg_path_str = pkg_path.to_string_lossy().to_string();
                    println!("Installing from repos: {name}");
                    return install(&pkg_path, Path::new("/"), &sync_dir.parent().unwrap_or(sync_dir).join("bulb.db"), &sync_dir.parent().unwrap_or(sync_dir).join("content"));
                }
            }
        }
    }

    println!("Installing from AUR: {name}");
    let pkg_url = bulb::aur::aur_download_url(name);

    let client = bulb::download::DownloadClient::new(cache_dir.clone(), 4)?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

    let pkg_path = rt.block_on(client.download(&pkg_url, None))?;
    drop(rt);

    let extract_dir = tempfile::tempdir()?;
    let tar_file = fs::File::open(&pkg_path)?;
    let decoder = flate2::read::GzDecoder::new(tar_file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(extract_dir.path())?;

    let source_dir = extract_dir.path().join(name);

    if !source_dir.exists() {
        let entries: Vec<_> = fs::read_dir(extract_dir.path())?.filter_map(|e| e.ok()).collect();
        if let Some(first) = entries.first() {
            if first.path().is_dir() {
                let source_dir = first.path().to_path_buf();
                return build_and_install_aur(&source_dir, name);
            }
        }
        return Err(BulbError::InvalidMetadata(format!("AUR package {name} has no PKGBUILD directory")));
    }

    build_and_install_aur(&source_dir, name)
}

fn build_and_install_aur(source_dir: &Path, pkg_name: &str) -> Result<()> {
    let pkgbuild_path = source_dir.join("PKGBUILD");
    if !pkgbuild_path.exists() {
        return Err(BulbError::InvalidMetadata(format!("{pkg_name}: no PKGBUILD found")));
    }

    let pkgbuild_content = fs::read_to_string(&pkgbuild_path)?;
    let pkg = bulb::format::aur::parse_pkgbuild(&pkgbuild_content)?;

    let temp = tempfile::tempdir()?;
    let tar_path = temp.path().join("package.tar");
    {
        let tar_file = fs::File::create(&tar_path)?;
        let mut builder = tar::Builder::new(tar_file);

        let version = if pkg.epoch.as_deref() == Some("0") || pkg.epoch.is_none() {
            format!("{}-{}", pkg.pkgver, pkg.pkgrel)
        } else if let Some(epoch) = &pkg.epoch {
            format!("{}:{}-{}", epoch, pkg.pkgver, pkg.pkgrel)
        } else {
            format!("{}-{}", pkg.pkgver, pkg.pkgrel)
        };

        let pkginfo = format!(
            "pkgname = {}\npkgver = {}\narch = {}\npkgdesc = {}\npackager = bulb\n",
            pkg.pkgname,
            version,
            pkg.arch.as_deref().unwrap_or("x86_64"),
            pkg.pkgdesc.as_deref().unwrap_or(""),
        );

        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(pkginfo.as_bytes().len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, ".PKGINFO", pkginfo.as_bytes())?;

        for entry in walkdir::WalkDir::new(source_dir).follow_links(false) {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(source_dir)?;
            if relative.as_os_str().is_empty() || relative == Path::new("PKGBUILD") {
                continue;
            }
            builder.append_path_with_name(path, relative)?;
        }
        builder.finish()?;
    }

    let output = source_dir.join(format!("{pkg_name}.pkg.tar.zst"));

    let mut encoder = zstd::stream::Encoder::new(
        fs::File::create(&output)?,
        19,
    )?;
    std::io::copy(&mut fs::File::open(&tar_path)?, &mut encoder)?;
    encoder.finish()?;

    println!("built {}", output.display());

    let db_path = std::path::PathBuf::from("/var/lib/bulb/bulb.db");
    let store_path = std::path::PathBuf::from("/var/lib/bulb/content");

    install(&output, Path::new("/"), &db_path, &store_path)
}

fn update_all(offline: bool, sync_dir: &Path, root: &Path, db_path: &Path, store_path: &Path) -> Result<()> {
    if offline {
        return Err(BulbError::Config("cannot update in offline mode".into()));
    }

    let conf = bulb::config::pacman_conf::PacmanConf::load(std::path::Path::new("/etc/pacman.conf"))?;
    let cache_dir = store_path.parent().unwrap_or(store_path).join("cache");
    fs::create_dir_all(&cache_dir)?;

    let db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
    let installed = db.list_installed(gen_id)?;

    if installed.is_empty() {
        println!("no packages installed");
        return Ok(());
    }

    let system_arch = std::env::consts::ARCH;
    let mut updated = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();

    for pkg in &installed {
        let mut found = false;
        for repo in &conf.repos {
            let repo_db_path = sync_dir.join(format!("{}.db", repo.name));
            if !repo_db_path.exists() {
                continue;
            }
            let pkgs = bulb::sync::SyncDb::parse_sync_db(&repo_db_path)?;
            for remote_pkg in &pkgs {
                if remote_pkg.name == pkg.name {
                    let remote_version = remote_pkg.version.to_string();
                    if remote_version != pkg.version {
                        println!("{}: {} -> {}", pkg.name, pkg.version, remote_version);

                        let filename = remote_pkg.filename.as_deref().unwrap_or("");
                        let mirror = repo.servers.first()
                            .cloned()
                            .unwrap_or_else(|| format!("https://mirror.rackspace.com/archlinux/{}", repo.name));
                        let mirror = mirror.replace("$repo", &repo.name).replace("$arch", system_arch);
                        let url = format!("{}/{}", mirror.trim_end_matches('/'), filename);

                        let client = bulb::download::DownloadClient::new(cache_dir.clone(), 4)?;
                        let rt = tokio::runtime::Runtime::new()
                            .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

                        match rt.block_on(client.download(&url, remote_pkg.sha256.as_deref())) {
                            Ok(pkg_path) => {
                                drop(rt);
                                match install(&pkg_path, root, db_path, store_path) {
                                    Ok(()) => updated += 1,
                                    Err(e) => errors.push(format!("{}: {e}", pkg.name)),
                                }
                            }
                            Err(e) => {
                                drop(rt);
                                errors.push(format!("{}: download failed: {e}", pkg.name));
                            }
                        }
                    } else {
                        skipped += 1;
                    }
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        if !found {
            skipped += 1;
        }
    }

    println!();
    println!("updated: {updated}, skipped: {skipped}, errors: {}", errors.len());

    for err in &errors {
        eprintln!("  error: {err}");
    }

    if !errors.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

fn query(
    package: Option<String>,
    info: bool,
    list: bool,
    owner: Option<PathBuf>,
    root: &Path,
    db_path: &Path,
) -> Result<()> {
    let db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;

    if let Some(owner_path) = owner {
        let owner_path = query_path_for_root(&owner_path, root);
        let Some(owner_name) = db.find_file_owner(gen_id, &owner_path)? else {
            return Err(BulbError::PackageNotFound(owner_path));
        };
        println!("{owner_name}");
        return Ok(());
    }

    let Some(package) = package else {
        let packages = db.list_installed(gen_id)?;
        for package in packages {
            println!("{} {}", package.name, package.version);
        }
        return Ok(());
    };

    let Some(package_info) = db.get_installed_package(gen_id, &package)? else {
        return Err(BulbError::PackageNotFound(package));
    };

    if list {
        let files = db.get_installed_files(gen_id, &package_info.name)?;
        for file in files {
            println!("{}", file.display());
        }
    } else if info {
        println!("Name           : {}", package_info.name);
        println!("Version        : {}", package_info.version);
        println!("Architecture   : {}", package_info.arch);
        if let Some(description) = &package_info.description {
            println!("Description    : {description}");
        }
        if let Some(url) = &package_info.url {
            println!("URL            : {url}");
        }
        if let Some(packager) = &package_info.packager {
            println!("Packager       : {packager}");
        }
        if !package_info.depends.is_empty() {
            println!(
                "Depends On     : {}",
                package_info
                    .depends
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    } else {
        println!("{} {}", package_info.name, package_info.version);
    }

    Ok(())
}

fn query_path_for_root(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn show_cache_status(cache_dir: &Path) -> Result<()> {
    if !cache_dir.exists() {
        println!("Cache directory does not exist: {}", cache_dir.display());
        return Ok(());
    }

    let mut total_size: u64 = 0;
    let mut file_count: u64 = 0;
    let mut packages: Vec<(String, u64)> = Vec::new();

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("zst") {
            let size = entry.metadata()?.len();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            packages.push((name.to_string(), size));
            total_size += size;
            file_count += 1;
        }
    }

    packages.sort_by(|a, b| a.0.cmp(&b.0));

    if packages.is_empty() {
        println!("Cache is empty: {}", cache_dir.display());
        return Ok(());
    }

    println!("Cache: {}", cache_dir.display());
    println!("Packages: {}", file_count);
    println!("Total size: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);
    println!();

    for (name, size) in &packages {
        println!("  {:.2} MB  {}", *size as f64 / 1024.0 / 1024.0, name);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_queries_local_package() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src");
        let root = temp.path().join("root");
        let db_path = temp.path().join("bulb.db");
        let store_path = temp.path().join("content");
        fs::create_dir_all(source.join("usr/bin")).unwrap();
        fs::create_dir_all(&root).unwrap();

        fs::write(
            source.join("Bulb.toml"),
            r#"
            [package]
            name = "hello"
            version = "1.0"
            release = "1"
            arch = "x86_64"
            desc = "Hello world"
            packager = "bulb test"
            "#,
        )
        .unwrap();
        fs::write(source.join("usr/bin/hello"), "#!/bin/sh\necho hello\n").unwrap();

        let package = temp.path().join("hello.pkg.tar.zst");
        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
            offline: false,
            command: Some(Commands::Build {
                source_dir: source,
                output: Some(package.clone()),
                no_sandbox: true,
            }),
            query: None,
        };
        run(cli).unwrap();
        assert!(package.exists());

        let cli = Cli {
                    root: root.clone(),
                    db_path: db_path.clone(),
                    store_path: store_path.clone(),
                    sync_dir: temp.path().join("sync").clone(),
                    offline: false,
                    command: Some(Commands::Install { package }),
                    query: None,
                };
        run(cli).unwrap();
        assert!(root.join("usr/bin/hello").exists());

        let cli = Cli {
                    root: root.clone(),
                    db_path: db_path.clone(),
                    store_path: store_path.clone(),
                    sync_dir: temp.path().join("sync").clone(),
                    offline: false,
                    command: Some(Commands::Query {
                        package: Some("hello".into()),
                        info: true,
                        list: false,
                        owner: None,
                    }),
                    query: None,
                };
        run(cli).unwrap();

        let cli = Cli {
            root: root.clone(),
            db_path,
            store_path,
            sync_dir: temp.path().join("sync"),
            offline: false,
            command: Some(Commands::Remove {
                package: "hello".into(),
            }),
            query: None,
        };
        run(cli).unwrap();
        assert!(!root.join("usr/bin/hello").exists());
    }

    #[test]
    fn content_store_deduplication() {
        let temp = tempfile::tempdir().unwrap();
        let store_dir = temp.path().join("content");
        let root = temp.path().join("root");
        fs::create_dir_all(&root).unwrap();

        let store = bulb::db::store::ContentStore::new(store_dir);
        store.init().unwrap();

        let data = b"identical content";
        let hash1 = store.add(data).unwrap();
        let hash2 = store.add(data).unwrap();
        assert_eq!(hash1, hash2);

        let dest1 = root.join("file1");
        let dest2 = root.join("file2");
        store.link(&hash1, &dest1).unwrap();
        store.link(&hash2, &dest2).unwrap();

        assert!(dest1.exists());
        assert!(dest2.exists());
        assert_eq!(fs::read(&dest1).unwrap(), data);
        assert_eq!(fs::read(&dest2).unwrap(), data);
    }

    #[test]
    fn generation_rollback_removes_files() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src");
        let root = temp.path().join("root");
        let db_path = temp.path().join("bulb.db");
        let store_path = temp.path().join("content");
        fs::create_dir_all(source.join("usr/bin")).unwrap();
        fs::create_dir_all(&root).unwrap();

        fs::write(
            source.join("Bulb.toml"),
            r#"
            [package]
            name = "testpkg"
            version = "1.0"
            release = "1"
            arch = "x86_64"
            desc = "Test package"
            packager = "bulb test"
            "#,
        )
        .unwrap();
        fs::write(source.join("usr/bin/testpkg"), "#!/bin/sh\necho test\n").unwrap();

        let package = temp.path().join("testpkg.pkg.tar.zst");
        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
            offline: false,
            command: Some(Commands::Build {
                source_dir: source,
                output: Some(package.clone()),
                no_sandbox: true,
            }),
            query: None,
        };
        run(cli).unwrap();

        let cli = Cli {
                    root: root.clone(),
                    db_path: db_path.clone(),
                    store_path: store_path.clone(),
                    sync_dir: temp.path().join("sync").clone(),
                    offline: false,
                    command: Some(Commands::Install { package }),
                    query: None,
                };
        run(cli).unwrap();
        assert!(root.join("usr/bin/testpkg").exists());

        let cli = Cli {
                    root: root.clone(),
                    db_path: db_path.clone(),
                    store_path: store_path.clone(),
                    sync_dir: temp.path().join("sync").clone(),
                    offline: false,
                    command: Some(Commands::Rollback),
                    query: None,
                };
        run(cli).unwrap();
        assert!(!root.join("usr/bin/testpkg").exists());
    }
}

