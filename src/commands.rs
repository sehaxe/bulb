use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use bulb::core::pkginfo::PackageInfo;
use bulb::db::Database;
use bulb::error::{BulbError, Result};
use bulb::format::native::package as native_pkg;

#[derive(Debug, Parser)]
#[command(name = "bulb", version, about = "A bzip3-based package manager")]
pub struct Cli {
    #[arg(short = 'r', long, default_value = "/")]
    pub root: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/bulb.db")]
    pub db_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/content")]
    pub store_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/sync")]
    pub sync_dir: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Install a local .pkg.tar.zst or .pkg.tar.bz3 package")]
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

    #[command(about = "Build a local .pkg.tar.bz3 package from a directory containing Bulb.toml")]
    Build {
        source_dir: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    #[command(about = "Build a package inside a sandbox (bwrap)")]
    BuildSandbox {
        source_dir: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        allow_network: bool,
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

    #[command(about = "Benchmark: decompress a .pkg.tar.bz3 or .pkg.tar.zst to a file")]
    BenchDecompress {
        package: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },

    #[command(about = "Benchmark: parse a sync database file")]
    BenchSyncParse {
        db_path: PathBuf,
    },

    #[command(about = "Benchmark: version comparison throughput")]
    BenchVercmp,

    #[command(about = "Interactive TUI with fuzzy search")]
    Tui,
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
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
        Commands::Build { source_dir, output } => {
            let manifest_path = source_dir.join("Bulb.toml");
            let manifest_text = fs::read_to_string(manifest_path)?;
            let manifest: bulb::format::native::manifest::BuildManifest = toml::from_str(&manifest_text)?;
            let info = native_pkg::manifest_to_pkginfo(&manifest);

            let output = output
                .map(PathBuf::from)
                .unwrap_or_else(|| source_dir.join(native_pkg::package_file_name(&info)));

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

            bzip3::stream::compress(
                fs::File::open(&tar_path)?,
                fs::File::create(&output)?,
                400 * 1024,
            )?;

            println!("built {}", output.display());
            Ok(())
        }
        Commands::BuildSandbox {
            source_dir,
            output,
            allow_network,
        } => {
            let output = output.unwrap_or_else(|| {
                let manifest_path = source_dir.join("Bulb.toml");
                if let Ok(manifest_text) = fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) =
                        toml::from_str::<bulb::format::native::manifest::BuildManifest>(&manifest_text)
                    {
                        let info = native_pkg::manifest_to_pkginfo(&manifest);
                        return source_dir.join(native_pkg::package_file_name(&info));
                    }
                }
                source_dir.join("output.pkg.tar.bz3")
            });

            let mut config = bulb::sandbox::SandboxConfig::new(source_dir, output);
            config.allow_network = allow_network;

            let result = bulb::sandbox::SandboxRunner::run(&config)?;
            println!("built (sandbox) {}", result.display());
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

            let pkg_path = rt.block_on(client.download(&url, None))?;
            drop(rt);

            install(&pkg_path, &cli.root, &cli.db_path, &cli.store_path)
        }
        Commands::Update => {
            println!("update not yet implemented");
            Ok(())
        }
        Commands::BenchDecompress { package, output } => {
            let file_name = package.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name.ends_with(".pkg.tar.bz3") {
                let compressed = fs::read(&package)?;
                let mut decompressed = Vec::with_capacity(compressed.len() * 3);
                bzip3::stream::parallel_decompress(&compressed[..], &mut decompressed)
                    .map_err(|e| BulbError::Decompress(e.to_string()))?;
                fs::write(&output, &decompressed)?;
            } else if file_name.ends_with(".pkg.tar.zst") {
                let compressed = fs::File::open(&package)?;
                let mut decoder = zstd::stream::Decoder::new(compressed)?;
                let mut out = fs::File::create(&output)?;
                std::io::copy(&mut decoder, &mut out)?;
            } else {
                return Err(BulbError::UnsupportedPackageFormat(package));
            }
            Ok(())
        }
        Commands::BenchSyncParse { db_path } => {
            let _pkgs = bulb::sync::SyncDb::parse_sync_db(&db_path)?;
            println!("parsed {} packages", _pkgs.len());
            Ok(())
        }
        Commands::BenchVercmp => {
            use bulb::core::version::BorrowedVersion;
            let versions = [
                "1.0", "2.0", "1.0.1", "1.0.2", "2.1.0", "0.9.9", "10.0.0",
                "1.0a", "1.0b", "1.0rc1", "1.0alpha", "1.0beta", "1.0pre1",
                "1.0.git20240101", "1.0-1", "2.0-2", "1.0.0.0",
                "1.2.3.4.5.6", "99.99.99", "0.0.1",
            ];
            let parsed: Vec<BorrowedVersion<'_>> = versions.iter()
                .map(|v| bulb::core::version::Version::parse_borrowed(v).unwrap())
                .collect();
            let pairs: Vec<(usize, usize)> = (0..parsed.len())
                .flat_map(|a| (0..parsed.len()).map(move |b| (a, b)))
                .collect();
            for _ in 0..50_000 {
                for &(a, b) in &pairs {
                    let _ = parsed[a].cmp_alpm(&parsed[b]);
                }
            }
            println!("{} comparisons", pairs.len() * 50_000);
            Ok(())
        }
        Commands::Tui => {
            bulb::tui::run_app(cli.root, cli.db_path, cli.store_path)
        }
    }
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
    } else if file_name.ends_with(".pkg.tar.bz3") {
        // Parallel decompression: bzip3 blocks are independent and can be
        // decompressed across rayon's thread pool simultaneously.
        let compressed = fs::read(package)?;
        let mut decompressed = Vec::with_capacity(compressed.len() * 3);
        bzip3::stream::parallel_decompress(&compressed[..], &mut decompressed)
            .map_err(|e| BulbError::Decompress(e.to_string()))?;
        let mut archive = tar::Archive::new(&decompressed[..]);
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

        let package = temp.path().join("hello.pkg.tar.bz3");
        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
                        command: Commands::Build {
                source_dir: source,
                output: Some(package.clone()),
            },
        };
        run(cli).unwrap();
        assert!(package.exists());

        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
                        command: Commands::Install { package },
        };
        run(cli).unwrap();
        assert!(root.join("usr/bin/hello").exists());

        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
                        command: Commands::Query {
                package: Some("hello".into()),
                info: true,
                list: false,
                owner: None,
            },
        };
        run(cli).unwrap();

        let cli = Cli {
            root: root.clone(),
            db_path,
            store_path,
            sync_dir: temp.path().join("sync"),
            command: Commands::Remove {
                package: "hello".into(),
            },
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

        let package = temp.path().join("testpkg.pkg.tar.bz3");
        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
                        command: Commands::Build {
                source_dir: source,
                output: Some(package.clone()),
            },
        };
        run(cli).unwrap();

        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
                        command: Commands::Install { package },
        };
        run(cli).unwrap();
        assert!(root.join("usr/bin/testpkg").exists());

        let cli = Cli {
            root: root.clone(),
            db_path: db_path.clone(),
            store_path: store_path.clone(),
            sync_dir: temp.path().join("sync").clone(),
                        command: Commands::Rollback,
        };
        run(cli).unwrap();
        assert!(!root.join("usr/bin/testpkg").exists());
    }
}

