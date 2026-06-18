use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use bulb::core::pkginfo::PackageInfo;
use bulb::db::Database;
use bulb::error::{BulbError, Result};
use bulb::format::native::package as native_pkg;

#[derive(Debug, Parser)]
#[command(
    name = "bulb",
    version,
    about = "A fast Arch Linux package manager",
    long_about = "bulb — a fast, atomic, and secure package manager for Arch Linux.\n\nSupports zstd compression, BLAKE3 content-addressed store with hardlink dedup,\ngenerational upgrades with instant rollback, parallel installs, and delta updates.\n\nBulb reads pacman.conf for repository configuration and is a drop-in replacement\nfor pacman/paru with a unified install command."
)]
pub struct Cli {
    #[arg(short = 'r', long, default_value = "/", help = "Filesystem root for chroot operations")]
    pub root: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/bulb.db", help = "SQLite database path")]
    pub db_path: PathBuf,

    #[arg(long, default_value = "/var/lib/bulb/content", help = "BLAKE3 content store path")]
    pub store_path: PathBuf,

    #[cfg(feature = "archlinux")]
    #[arg(long, default_value = "/var/lib/bulb/sync", help = "Sync database directory")]
    pub sync_dir: PathBuf,

    #[arg(long, help = "Offline mode: use cached packages only")]
    pub offline: bool,

    #[arg(short = 'y', long, help = "Skip all confirmation prompts")]
    pub noconfirm: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[cfg(feature = "archlinux")]
    #[arg(help = "Package name or search query (launches interactive TUI)")]
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
        #[arg(short = 'v', long, default_value = "1.0", help = "Initial version")]
        version: String,
    },

    #[command(about = "Install packages (files or names)", long_about = "Install local .pkg.tar.zst files or resolve package names from sync repositories.\n\nExamples:\n  bulb install ./package.pkg.tar.zst\n  bulb install firefox vim git\n  bulb install --needed base-devel\n  bulb install --force --noconfirm package")]
    Install {
        #[arg(required = true, help = "Package files (.pkg.tar.zst) or package names")]
        targets: Vec<String>,
        #[arg(long, help = "Force reinstall even if same version is installed")]
        force: bool,
        #[arg(long, help = "Skip if already installed at same or newer version")]
        needed: bool,
        #[arg(long, help = "Download packages without installing")]
        download_only: bool,
    },

    #[command(about = "Remove an installed package")]
    Remove {
        #[arg(help = "Package name to remove")]
        package: String,
        #[arg(short = 'n', long, help = "Also remove dependencies not needed by other packages")]
        recursive: bool,
        #[arg(long, help = "Remove unneeded dependencies (use with --recursive)")]
        nosave: bool,
    },

    #[command(about = "Query installed packages")]
    Query {
        #[arg(help = "Package name to query")]
        package: Option<String>,
        #[arg(short, long, help = "Show detailed package information")]
        info: bool,
        #[arg(short = 'l', long, help = "List files owned by the package")]
        list: bool,
        #[arg(short = 'o', long, value_name = "PATH", help = "Find which package owns a file")]
        owner: Option<PathBuf>,
        #[cfg(feature = "archlinux")]
        #[arg(short = 'm', long, help = "List foreign packages (not in any sync DB)")]
        foreign: bool,
        #[arg(short = 't', long, help = "List unneeded/orphaned packages")]
        unneeded: bool,
        #[cfg(feature = "archlinux")]
        #[arg(short = 'u', long, help = "List packages with newer versions available")]
        upgradable: bool,
        #[arg(short = 's', long, help = "Search installed packages by name or description")]
        search: Option<String>,
        #[arg(long, help = "Show install reason (explicit or dependency)")]
        reasons: bool,
    },

    #[command(about = "Build a .pkg.tar.zst from a directory containing Bulb.toml")]
    Build {
        #[arg(help = "Source directory containing Bulb.toml")]
        source_dir: PathBuf,
        #[arg(short, long, help = "Output path for the built package")]
        output: Option<PathBuf>,
        #[arg(long, help = "Build without bwrap sandbox (requires root)")]
        no_sandbox: bool,
    },

    #[cfg(feature = "archlinux")]
    #[command(about = "Parse and display a PKGBUILD")]
    ParsePkgbuild {
        #[arg(help = "Path to PKGBUILD file")]
        path: PathBuf,
    },

    #[cfg(feature = "archlinux")]
    #[command(about = "Migrate from pacman local database")]
    Migrate {
        #[arg(long, default_value = "/var/lib/pacman/local", help = "Pacman local DB path")]
        pacman_local: PathBuf,
    },

    #[cfg(feature = "archlinux")]
    #[command(about = "Show info about a sync package")]
    SyncInfo {
        #[arg(required = true, help = "Package name(s) to query")]
        packages: Vec<String>,
    },

    #[cfg(feature = "archlinux")]
    #[command(about = "List packages in a repository")]
    RepoList {
        #[arg(help = "Repository name (default: list all repos)")]
        repo: Option<String>,
    },

    #[command(about = "List generations")]
    ListGenerations,

    #[command(about = "Switch to a specific generation")]
    Switch {
        #[arg(help = "Generation number to switch to")]
        generation: i64,
    },

    #[command(about = "Rollback to the previous generation")]
    Rollback,

    #[cfg(feature = "archlinux")]
    #[command(about = "Sync package databases from mirrors")]
    Sync,

    #[cfg(feature = "archlinux")]
    #[command(about = "Update all installed packages")]
    Update,

    #[command(about = "Start the bulb daemon (Unix socket IPC)")]
    Daemon,

    #[command(about = "Manage package cache")]
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
    },

    #[command(about = "Generate shell completions", long_about = "Generate shell completion scripts for bash, zsh, or fish.\n\nUsage:\n  bulb completions bash > /usr/share/bash-completion/completions/bulb\n  bulb completions zsh > /usr/share/zsh/site-functions/_bulb\n  bulb completions fish > ~/.config/fish/completions/bulb.fish", hide = true)]
    Completions {
        #[arg(value_enum, help = "Shell to generate completions for")]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Subcommand)]
pub enum CacheAction {
    #[command(about = "List cached packages")]
    List,
    #[command(about = "Remove old cached packages, keeping latest N versions")]
    Clean {
        #[arg(short = 'k', long, default_value = "1", help = "Number of versions to keep per package")]
        keep: usize,
    },
    #[command(about = "Remove ALL cached packages")]
    CleanAll,
    #[command(about = "Show total cache size in human-readable format")]
    Size,
}

pub fn run(cli: Cli) -> Result<()> {
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            #[cfg(feature = "archlinux")]
            if let Some(query) = &cli.query {
                return run_search_and_select(
                    &[query.clone()],
                    &cli.root,
                    &cli.db_path,
                    &cli.store_path,
                    &cli.sync_dir,
                );
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
        Commands::Install { targets, force, needed, download_only } => {
            let local: Vec<&str> = targets.iter()
                .filter(|t| t.ends_with(".pkg.tar.zst"))
                .map(|s| s.as_str())
                .collect();
            let names: Vec<String> = targets.iter()
                .filter(|t| !t.ends_with(".pkg.tar.zst"))
                .cloned()
                .collect();

            let _ = (&force, &needed, &download_only);

            if !local.is_empty() {
                for target in &local {
                    install(Path::new(target), &cli.root, &cli.db_path, &cli.store_path)?;
                }
            }
            if !names.is_empty() {
                #[cfg(feature = "archlinux")]
                {
                    install_names(&names, cli.noconfirm, force, needed, download_only, cli.offline, &cli.root, &cli.db_path, &cli.store_path, &cli.sync_dir)?;
                }
                #[cfg(not(feature = "archlinux"))]
                {
                    return Err(BulbError::Config(
                        "installing by name requires archlinux feature. Install .pkg.tar.zst files directly.".into()
                    ));
                }
            }
            Ok(())
        }
        Commands::Remove { package, recursive, nosave } => {
            if recursive {
                remove_with_deps(&package, nosave, cli.noconfirm, &cli.root, &cli.db_path, &cli.store_path)
            } else {
                remove(&package, cli.noconfirm, &cli.root, &cli.db_path)
            }
        }
        Commands::Query {
            package,
            info,
            list,
            owner,
            #[cfg(feature = "archlinux")]
            foreign,
            unneeded,
            #[cfg(feature = "archlinux")]
            upgradable,
            search,
            reasons,
        } => {
            #[cfg(feature = "archlinux")]
            if foreign {
                return query_foreign(&cli.root, &cli.db_path, &cli.sync_dir);
            }
            if unneeded {
                return query_orphans(&cli.root, &cli.db_path);
            }
            #[cfg(feature = "archlinux")]
            if upgradable {
                return query_upgradable(&cli.root, &cli.db_path, &cli.store_path, &cli.sync_dir);
            }
            if let Some(q) = search {
                return query_search(&q, &cli.root, &cli.db_path);
            }
            query(package, info, list, owner, reasons, &cli.root, &cli.db_path)
        }
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
        #[cfg(feature = "archlinux")]
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
        #[cfg(feature = "archlinux")]
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
        #[cfg(feature = "archlinux")]
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

            let hooks_dir = bulb::hooks::default_hooks_dir(&cli.root);
            if !hooks_dir.exists() || fs::read_dir(&hooks_dir).map(|mut e| e.next().is_none()).unwrap_or(true) {
                bulb::hooks::install_default_hooks(&hooks_dir)?;
                println!("installed default hooks");
            }

            Ok(())
        }
        #[cfg(feature = "archlinux")]
        Commands::Update => update_all(cli.offline, &cli.sync_dir, &cli.root, &cli.db_path, &cli.store_path),
        #[cfg(feature = "archlinux")]
        Commands::SyncInfo { packages } => query_sync_info(&packages, &cli.sync_dir),
        #[cfg(feature = "archlinux")]
        Commands::RepoList { repo } => query_repo_list(repo.as_deref(), &cli.sync_dir),
        Commands::Cache { action } => {
            let cache_dir = cli.store_path.parent().unwrap_or(&cli.store_path).join("cache");
            match action {
                None | Some(CacheAction::List) => show_cache_status(&cache_dir),
                Some(CacheAction::Clean { keep }) => clean_cache(&cache_dir, keep),
                Some(CacheAction::CleanAll) => clean_all_cache(&cache_dir),
                Some(CacheAction::Size) => show_cache_size(&cache_dir),
            }
        }
        Commands::Daemon => {
            let daemon_dir = cli.store_path.parent().unwrap_or(&cli.store_path);
            fs::create_dir_all(daemon_dir)?;
            let config = bulb::daemon::DaemonConfig {
                socket_path: daemon_dir.join("bulbd.sock"),
                pid_path: daemon_dir.join("bulbd.pid"),
                db_path: cli.db_path.clone(),
                store_path: cli.store_path.clone(),
                cache_path: cli.store_path.parent().unwrap_or(&cli.store_path).join("cache"),
                max_cache_size: 2 * 1024 * 1024 * 1024,
            };
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;
            rt.block_on(bulb::daemon::run_daemon(config))
        }
        Commands::Completions { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            clap_complete::generate(shell, &mut cmd, "bulb", &mut std::io::stdout());
            Ok(())
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
    use bulb::lock::Lock;
    let _lock = Lock::acquire(root)?;

    let file_name = package.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let sig_path = package.with_extension("sig");
    if sig_path.exists() {
        let verifier = bulb::pgp::PgpVerifier::new();
        match verifier.verify(package, &sig_path) {
            Ok(bulb::pgp::VerifyResult::Valid { key_id }) => {
                if let Some(id) = key_id {
                    println!("  signature verified (key: {})", &id[..8.min(id.len())]);
                }
            }
            Err(e) => {
                return Err(e);
            }
            _ => {}
        }
    }

    let mut db = Database::open(db_path)?;
    let gen_id = db.ensure_generation()?;
    let is_upgrade = db.get_installed_package(gen_id, &extract_pkg_name_from_path(package)?).ok().flatten().is_some();

    let store = bulb::db::store::ContentStore::new(store_path.to_path_buf());
    store.init()?;

    let (info, extracted_files, install_script_content) = if file_name.ends_with(".pkg.tar.zst") {
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

    let hooks_dir = bulb::hooks::default_hooks_dir(root);
    let install_script = install_script_content
        .as_deref()
        .map(bulb::hooks::InstallScript::parse);

    if let Some(script) = &install_script {
        if script.has_any() {
            script.run_pre(&info.name, root, is_upgrade)?;
        }
    }

    let old_files = if is_upgrade {
        db.get_installed_files(gen_id, &info.name)?
    } else {
        Vec::new()
    };

    let backup_files = info.backup.clone();
    let user_modified = bulb::hooks::detect_user_modified(root, &backup_files, &old_files);
    bulb::hooks::save_backup_files(root, &backup_files)?;

    let new_gen = db.create_generation(&format!("install {}", info.name))?;
    db.insert_installed_package(new_gen, &info, &extracted_files, &format!("installed-{}", info.name))?;

    let pacnew_files = bulb::hooks::handle_pacnew(root, &info.name, &backup_files, &user_modified)?;
    for pf in &pacnew_files {
        println!("  {}: {} saved as .pacnew", info.name, pf.original.display());
    }

    let operation = if is_upgrade {
        bulb::hooks::HookOperation::Upgrade
    } else {
        bulb::hooks::HookOperation::Install
    };
    let _ = bulb::hooks::run_system_hooks(&hooks_dir, &info.name, operation, bulb::hooks::HookWhen::PreTransaction, root);

    if let Some(script) = &install_script {
        if script.has_any() {
            script.run_post(&info.name, root, is_upgrade)?;
        }
    }

    let _ = bulb::hooks::run_system_hooks(&hooks_dir, &info.name, bulb::hooks::HookOperation::Install, bulb::hooks::HookWhen::PostTransaction, root);

    println!("installed {} {}", info.name, info.version);
    Ok(())
}

fn extract_pkg_name_from_path(package: &Path) -> Result<String> {
    let file_name = package.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let name = file_name
        .strip_suffix(".pkg.tar.zst")
        .unwrap_or(file_name);
    let name = name.rsplitn(3, '-').last().unwrap_or(name);
    Ok(name.to_string())
}

fn single_pass_extract<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    root: &Path,
    store: &bulb::db::store::ContentStore,
) -> Result<(PackageInfo, Vec<PathBuf>, Option<String>)> {
    let mut pkginfo_text = None;
    let mut install_script = None;
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
            Some("install") => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                install_script = Some(text);
            }
            Some(".BUILDINFO") | Some(".MTREE") => {}
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
                        ensure_parent_dir(&dest, root, &mut created_dirs)?;
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
                        ensure_parent_dir(&dest, root, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let _ = fs::remove_file(&dest);
                            #[cfg(unix)]
                            std::os::unix::fs::symlink(&link_target, &dest)?;
                        }
                    }
                    tar::EntryType::Link => {
                        ensure_parent_dir(&dest, root, &mut created_dirs)?;
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

    Ok((info, files, install_script))
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

/// Prompt user to confirm. Returns true if proceed. Skips prompt if noconfirm or non-TTY.
fn confirm_proceed(message: &str, noconfirm: bool) -> bool {
    if noconfirm {
        return true;
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return true;
    }
    print!("{message} [Y/n] ");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    let trimmed = input.trim().to_lowercase();
    trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
}

fn find_cached_old_version(cache_dir: &Path, pkg_name: &str) -> Option<PathBuf> {
    let prefix = format!("{pkg_name}-");
    for entry in fs::read_dir(cache_dir).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(".pkg.tar.zst") {
            return Some(entry.path());
        }
    }
    None
}

fn apply_cached_delta(old_pkg: &Path, delta_path: &Path, output_path: &Path) -> Result<()> {
    if !delta_path.exists() {
        return Err(BulbError::Delta("delta file not found".into()));
    }

    let delta_data = fs::read(delta_path)?;
    const BLAKE3_SIZE: usize = 32;

    if delta_data.len() < BLAKE3_SIZE * 2 {
        return Err(BulbError::Delta("delta file too small".into()));
    }

    let expected_old = blake3::Hash::from_bytes(delta_data[..BLAKE3_SIZE].try_into().unwrap());
    let old_bytes = fs::read(old_pkg)?;
    let actual_old = blake3::hash(&old_bytes);
    if expected_old != actual_old {
        return Err(BulbError::Delta(format!(
            "old package hash mismatch: expected {}, got {}",
            expected_old.to_hex(),
            actual_old.to_hex()
        )));
    }

    let bsdiff_data = &delta_data[BLAKE3_SIZE * 2..];
    let mut new_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(bsdiff_data);
    bsdiff::patch(&old_bytes, &mut cursor, &mut new_bytes)
        .map_err(|e| BulbError::Delta(format!("bspatch failed: {e}")))?;

    let expected_new = blake3::Hash::from_bytes(
        delta_data[BLAKE3_SIZE..BLAKE3_SIZE * 2].try_into().unwrap()
    );
    let actual_new = blake3::hash(&new_bytes);
    if expected_new != actual_new {
        return Err(BulbError::Delta(format!(
            "new package hash mismatch: expected {}, got {}",
            expected_new.to_hex(),
            actual_new.to_hex()
        )));
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, &new_bytes)?;

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

fn remove(package: &str, noconfirm: bool, root: &Path, db_path: &Path) -> Result<()> {
    if !confirm_proceed(&format!("Remove {package}?"), noconfirm) {
        println!("aborted.");
        return Ok(());
    }

    use bulb::lock::Lock;
    let _lock = Lock::acquire(root)?;

    let mut db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;

    let files = db.get_installed_files(gen_id, package)?;
    let info = db.get_installed_package(gen_id, package)?
        .ok_or_else(|| BulbError::PackageNotFound(package.into()))?;

    let hooks_dir = bulb::hooks::default_hooks_dir(root);

    let _ = bulb::hooks::run_system_hooks(&hooks_dir, package, bulb::hooks::HookOperation::Remove, bulb::hooks::HookWhen::PreTransaction, root);

    let backup_files = info.backup.clone();
    bulb::hooks::handle_pacsave(root, package, &files, &backup_files)?;

    let new_gen = db.create_generation(&format!("remove {package}"))?;
    db.remove_package(new_gen, package)?;

    for file in files.iter().rev() {
        let path = root.join(file);
        if path.is_dir() {
            let _ = fs::remove_dir(&path);
        } else if path.is_file() || fs::symlink_metadata(&path).is_ok() {
            let _ = fs::remove_file(&path);
        }
    }

    let _ = bulb::hooks::run_system_hooks(&hooks_dir, package, bulb::hooks::HookOperation::Remove, bulb::hooks::HookWhen::PostTransaction, root);

    println!("removed {} {}", info.name, info.version);
    Ok(())
}

fn remove_with_deps(package: &str, nosave: bool, noconfirm: bool, root: &Path, db_path: &Path, _store_path: &Path) -> Result<()> {
    if !confirm_proceed(&format!("Remove {package} and its dependencies?"), noconfirm) {
        println!("aborted.");
        return Ok(());
    }

    use bulb::lock::Lock;
    let _lock = Lock::acquire(root)?;

    let mut db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;

    let installed = db.list_installed(gen_id)?;
    let mut to_remove = vec![package.to_string()];
    let mut visited = std::collections::HashSet::new();
    visited.insert(package.to_string());

    fn find_reverse_deps(pkg: &str, installed: &[PackageInfo]) -> Vec<String> {
        installed.iter()
            .filter(|p| p.depends.iter().any(|d| d.to_string() == pkg))
            .map(|p| p.name.clone())
            .collect()
    }

    let mut queue = vec![package.to_string()];
    while let Some(current) = queue.pop() {
        let rdeps = find_reverse_deps(&current, &installed);
        for rdep in rdeps {
            if !visited.contains(&rdep) {
                visited.insert(rdep.clone());
                to_remove.push(rdep.clone());
                queue.push(rdep);
            }
        }
    }

    let mut reverse_deps_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pkg in &installed {
        for dep in &pkg.depends {
            *reverse_deps_count.entry(dep.to_string()).or_insert(0) += 1;
        }
    }

    let mut actually_remove = Vec::new();
    let mut keep = Vec::new();

    for pkg_name in &to_remove {
        if pkg_name == package {
            actually_remove.push(pkg_name.clone());
            continue;
        }
        let rdep_count = reverse_deps_count.get(pkg_name).copied().unwrap_or(0);
        if rdep_count <= 1 {
            actually_remove.push(pkg_name.clone());
        } else {
            keep.push(pkg_name.clone());
        }
    }

    if !keep.is_empty() {
        println!("keeping dependencies needed by other packages:");
        for k in &keep {
            println!("  {k}");
        }
    }

    let hooks_dir = bulb::hooks::default_hooks_dir(root);
    let mut removed_count = 0;

    for pkg_name in &actually_remove {
        let files = db.get_installed_files(gen_id, pkg_name)?;
        let info = db.get_installed_package(gen_id, pkg_name)?
            .ok_or_else(|| BulbError::PackageNotFound(pkg_name.clone()))?;

        let _ = bulb::hooks::run_system_hooks(&hooks_dir, pkg_name, bulb::hooks::HookOperation::Remove, bulb::hooks::HookWhen::PreTransaction, root);

        let backup_files = info.backup.clone();
        if nosave {
            for file in files.iter().rev() {
                let path = root.join(file);
                let backup_path = path.with_extension("pacsave");
                if backup_path.exists() {
                    fs::remove_file(&backup_path)?;
                }
            }
        } else {
            let _ = bulb::hooks::handle_pacsave(root, pkg_name, &files, &backup_files);
        }

        let new_gen = db.create_generation(&format!("remove {pkg_name}"))?;
        db.remove_package(new_gen, pkg_name)?;

        for file in files.iter().rev() {
            let path = root.join(file);
            if path.is_file() || fs::symlink_metadata(&path).is_ok() {
                fs::remove_file(&path)?;
            } else if path.is_dir() {
                let _ = fs::remove_dir(&path);
            }
        }

        let _ = bulb::hooks::run_system_hooks(&hooks_dir, pkg_name, bulb::hooks::HookOperation::Remove, bulb::hooks::HookWhen::PostTransaction, root);

        println!("removed {} {}", info.name, info.version);
        removed_count += 1;
    }

    if removed_count > 1 {
        println!("removed {removed_count} packages total");
    }

    Ok(())
}

#[cfg(feature = "archlinux")]
fn run_search_and_select(
    queries: &[String],
    root: &PathBuf,
    db_path: &PathBuf,
    store_path: &PathBuf,
    sync_dir: &PathBuf,
) -> Result<()> {
    use bulb::tui::multi_select::{self, SearchResult};

    let mut results: Vec<SearchResult> = Vec::new();

    for query in queries {
        let query_lower = query.to_lowercase();

        if let Ok(entries) = fs::read_dir(sync_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("db") {
                    if let Ok(pkgs) = bulb::sync::SyncDb::parse_sync_db(&path) {
                        let repo = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                        for pkg in &pkgs {
                            if pkg.name.to_lowercase().contains(&query_lower)
                                || pkg.description.as_deref().map_or(false, |d| d.to_lowercase().contains(&query_lower))
                            {
                                results.push(SearchResult {
                                    repo: repo.to_string(),
                                    name: pkg.name.clone(),
                                    version: pkg.version.to_string(),
                                    description: pkg.description.clone().unwrap_or_default(),
                                    selected: pkg.name.to_lowercase() == query_lower,
                                });
                            }
                        }
                    }
                }
            }
        }

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

        match rt.block_on(bulb::aur::search_aur(query)) {
            Ok(aur_results) => {
                for pkg in aur_results {
                    results.push(SearchResult {
                        repo: "aur".into(),
                        name: pkg.name.clone(),
                        version: pkg.version,
                        description: pkg.description.unwrap_or_default(),
                        selected: pkg.name.to_lowercase() == query_lower,
                    });
                }
            }
            Err(e) => {
                eprintln!("AUR search failed: {e}");
            }
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results.dedup_by(|a, b| a.name == b.name);

    let root_clone = root.clone();
    let db_clone = db_path.clone();
    let store_clone = store_path.clone();
    let sync_clone = sync_dir.clone();

    multi_select::run_multi_select(results, move |selected| {
        install_names(&selected, true, false, false, false, false, &root_clone, &db_clone, &store_clone, &sync_clone)
    })
}

#[cfg(feature = "archlinux")]
fn install_names(
    names: &[String],
    noconfirm: bool,
    force: bool,
    needed: bool,
    download_only: bool,
    _offline: bool,
    root: &Path,
    db_path: &Path,
    store_path: &Path,
    sync_dir: &Path,
) -> Result<()> {
    use bulb::resolver::{Resolver, PackageVersion};
    use bulb::core::dependency::Depend;
    use bulb::core::version::Version;
    use bulb::lock::Lock;

    let _lock = Lock::acquire(root)?;

    let conf = bulb::config::pacman_conf::PacmanConf::load(std::path::Path::new("/etc/pacman.conf"))?;
    let cache_dir = store_path.parent().unwrap_or(store_path).join("cache");
    fs::create_dir_all(&cache_dir)?;

    let ignore_set: std::collections::HashSet<&str> = conf.options.ignore_pkg.iter().map(|s| s.as_str()).collect();
    let ignore_group_set: std::collections::HashSet<&str> = conf.options.ignore_group.iter().map(|s| s.as_str()).collect();

    let mut resolver = Resolver::new();

    let db = Database::open(db_path)?;
    let gen_id = db.current_generation().unwrap_or(None);
    if let Some(gen_id) = gen_id {
        let installed = db.list_installed(gen_id)?;
        for pkg in &installed {
            if let Ok(v) = Version::parse(&pkg.version) {
                resolver.add_installed(&pkg.name, v);
            }
        }
    }

    let system_arch = std::env::consts::ARCH;
    let mut all_packages: Vec<(String, bulb::sync::SyncPackage, String)> = Vec::new();

    for repo in &conf.repos {
        let repo_db_path = sync_dir.join(format!("{}.db", repo.name));
        if !repo_db_path.exists() {
            continue;
        }
        let pkgs = bulb::sync::SyncDb::parse_sync_db(&repo_db_path)?;
        for pkg in &pkgs {
            if pkg.arch != system_arch && pkg.arch != "any" {
                continue;
            }
            let filename = pkg.filename.clone().unwrap_or_default();
            resolver.add_package(&pkg.name, PackageVersion {
                version: pkg.version.clone(),
                depends: pkg.deps.iter().map(|d| Depend {
                    name: d.split('<').next()
                        .and_then(|s| s.split('>').next())
                        .and_then(|s| s.split('=').next())
                        .unwrap_or(d)
                        .to_string(),
                    constraint: Default::default(),
                    reason: None,
                }).collect(),
                provides: pkg.provides.clone(),
                groups: pkg.groups.clone(),
                replaces: pkg.replaces.clone(),
                conflicts: pkg.conflicts.clone(),
                filename: filename.clone(),
                repo: repo.name.clone(),
            });
            all_packages.push((repo.name.clone(), pkg.clone(), filename));
        }
    }

    let mut filtered_targets = Vec::new();
    for name in names {
        if ignore_set.contains(name.as_str()) {
            eprintln!("warning: {name} is in IgnorePkg — skipping");
            continue;
        }
        if ignore_group_set.contains(name.as_str()) {
            eprintln!("warning: {name} is in IgnoreGroup — skipping");
            continue;
        }
        filtered_targets.push(name.clone());
    }

    if filtered_targets.is_empty() {
        return Ok(());
    }

    let resolved = resolver.resolve(&filtered_targets)?;

    let mut to_download: Vec<(String, String, String, Option<String>)> = Vec::new();
    let mut to_install: Vec<&bulb::resolver::ResolvedPackage> = Vec::new();

    for r in &resolved {
        let already_installed = resolver.is_installed(&r.name);

        if already_installed && !force {
            if needed {
                continue;
            }
        }

        to_install.push(r);

        let cache_path = cache_dir.join(&r.filename);
        if cache_path.exists() && !force {
            println!("  {} (cached)", r.name);
            continue;
        }

        let (_, sync_pkg, _) = all_packages.iter()
            .find(|(_, p, _)| p.name == r.name)
            .ok_or_else(|| BulbError::PackageNotFound(r.name.clone()))?;

        let mirror = conf.repos.iter()
            .find(|rp| rp.name == r.repo)
            .and_then(|rp| rp.servers.first())
            .cloned()
            .unwrap_or_else(|| format!("https://mirror.rackspace.com/archlinux/{}", r.repo));
        let mirror = mirror.replace("$repo", &r.repo).replace("$arch", system_arch);
        let url = format!("{}/{}", mirror.trim_end_matches('/'), r.filename);
        let sha256 = sync_pkg.sha256.clone();

        to_download.push((r.name.clone(), url, r.filename.clone(), sha256));
    }

    if to_download.is_empty() && !force {
        println!("nothing to do");
        return Ok(());
    }

    if !to_download.is_empty() {
        let total = to_download.len();
        println!("resolving {total} dependencies...");

        let optdeps: Vec<(&str, Vec<&str>)> = resolved.iter()
            .filter_map(|r| {
                all_packages.iter()
                    .find(|(_, p, _)| p.name == r.name)
                    .map(|(_, p, _)| (p.name.as_str(), p.optdeps.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
            })
            .filter(|(_, deps)| !deps.is_empty())
            .collect();

        if !optdeps.is_empty() && !noconfirm {
            println!("\noptional dependencies for installed packages:");
            for (pkg, deps) in &optdeps {
                for dep in deps {
                    let dep_name = dep.split('<').next()
                        .and_then(|s| s.split('>').next())
                        .and_then(|s| s.split('=').next())
                        .unwrap_or(dep);
                    let installed = resolver.is_installed(dep_name);
                    let marker = if installed { " [installed]" } else { "" };
                    println!("  {pkg}: {dep}{marker}");
                }
            }
        }

        if !noconfirm {
            let pkg_names: Vec<&str> = to_install.iter().map(|r| r.name.as_str()).collect();
            let msg = format!("Proceed with installation of {}?", pkg_names.join(", "));
            if !confirm_proceed(&msg, noconfirm) {
                println!("aborted.");
                return Ok(());
            }
        }

        let client = bulb::download::DownloadClient::new(cache_dir.clone(), 4)?;
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| BulbError::Config(format!("tokio runtime: {e}")))?;

        for (i, (name, url, _filename, sha256)) in to_download.iter().enumerate() {
            println!("  [{}/{}] downloading {name}...", i + 1, total);

            let delta_url = format!("{url}.delta");
            let delta_path = cache_dir.join(format!("{}.delta", _filename));
            let pkg_path = cache_dir.join(_filename);

            let used_delta = if rt.block_on(client.download(&delta_url, None)).is_ok() {
                if let Some(old_pkg) = find_cached_old_version(&cache_dir, name) {
                    if let Ok(()) = apply_cached_delta(&old_pkg, &delta_path, &pkg_path) {
                        println!("  {name}: applied delta update");
                        true
                    } else {
                        let _ = fs::remove_file(&delta_path);
                        false
                    }
                } else {
                    let _ = fs::remove_file(&delta_path);
                    false
                }
            } else {
                false
            };

            if !used_delta {
                match rt.block_on(client.download(url, sha256.as_deref())) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("  failed to download {name}: {e}");
                        return Err(e);
                    }
                }
            }

            let sig_url = format!("{url}.sig");
            let _ = rt.block_on(client.download(&sig_url, None));
        }
        drop(rt);
    }

    if download_only {
        println!("packages downloaded (not installed)");
        return Ok(());
    }

    for r in &to_install {
        let already_installed = resolver.is_installed(&r.name);
        if already_installed && !force {
            continue;
        }

        let pkg_path = cache_dir.join(&r.filename);
        if pkg_path.exists() {
            install(&pkg_path, root, db_path, store_path)?;
        }
    }

    Ok(())
}

#[cfg(feature = "archlinux")]
#[allow(dead_code)]
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

#[cfg(feature = "archlinux")]
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
    reasons: bool,
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
            if reasons {
                println!("{} {} explicit", package.name, package.version);
            } else {
                println!("{} {}", package.name, package.version);
            }
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
        if !package_info.optdepends.is_empty() {
            println!(
                "Optional Deps  : {}",
                package_info
                    .optdepends
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        if !package_info.provides.is_empty() {
            println!(
                "Provides       : {}",
                package_info
                    .provides
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        if !package_info.conflicts.is_empty() {
            println!(
                "Conflicts With : {}",
                package_info
                    .conflicts
                    .iter()
                    .map(|c| c.to_string())
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
        println!("Cache is empty: {}", cache_dir.display());
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
    println!("Packages: {file_count}");
    println!("Total size: {:.2} MB", total_size as f64 / 1048576.0);
    println!();

    for (name, size) in &packages {
        println!("  {:.2} MB  {}", *size as f64 / 1048576.0, name);
    }

    Ok(())
}

fn parse_pkg_name_from_filename(filename: &str) -> String {
    let stem = filename.strip_suffix(".pkg.tar.zst").unwrap_or(filename);
    let parts: Vec<&str> = stem.rsplitn(3, '-').collect();
    if parts.len() == 3 {
        parts[2].to_string()
    } else {
        stem.to_string()
    }
}

fn clean_cache(cache_dir: &Path, keep: usize) -> Result<()> {
    if !cache_dir.exists() {
        println!("Cache is empty");
        return Ok(());
    }

    let mut packages: Vec<(String, PathBuf, u64)> = Vec::new();

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("zst") {
            let size = entry.metadata()?.len();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            packages.push((name.to_string(), path, size));
        }
    }

    if packages.is_empty() {
        println!("Cache is empty");
        return Ok(());
    }

    use std::collections::BTreeMap;
    let mut grouped: BTreeMap<String, Vec<(String, PathBuf, u64)>> = BTreeMap::new();

    for pkg @ (name, _, _) in &packages {
        let pkg_name = parse_pkg_name_from_filename(name);
        grouped.entry(pkg_name).or_default().push(pkg.clone());
    }

    let mut removed = 0u64;
    let mut freed = 0u64;

    for versions in grouped.values_mut() {
        versions.sort_by(|a, b| a.0.cmp(&b.0).reverse());

        for (_, path, size) in versions.iter().skip(keep) {
            let _ = fs::remove_file(path);
            removed += 1;
            freed += size;
            println!("removed {}", path.file_name().and_then(|n| n.to_str()).unwrap_or("?"));
        }
    }

    if removed == 0 {
        println!("cache is clean, nothing to remove");
    } else {
        println!("removed {removed} packages, freed {:.2} MB", freed as f64 / 1048576.0);
    }
    Ok(())
}

fn show_cache_size(cache_dir: &Path) -> Result<()> {
    if !cache_dir.exists() {
        println!("0 MB");
        return Ok(());
    }

    let mut total_size: u64 = 0;
    let mut file_count: u64 = 0;

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("zst") {
            total_size += entry.metadata()?.len();
            file_count += 1;
        }
    }

    println!("{:.2} MB ({file_count} packages)", total_size as f64 / 1048576.0);
    Ok(())
}

fn clean_all_cache(cache_dir: &Path) -> Result<()> {
    if !cache_dir.exists() {
        println!("Cache is empty");
        return Ok(());
    }

    let mut packages: Vec<(String, PathBuf, u64)> = Vec::new();

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("zst") {
            let size = entry.metadata()?.len();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            packages.push((name.to_string(), path, size));
        }
    }

    if packages.is_empty() {
        println!("Cache is empty");
        return Ok(());
    }

    let mut freed = 0u64;

    for (name, path, size) in &packages {
        let _ = fs::remove_file(path);
        freed += size;
        println!("removed {name}");
    }

    println!("removed {} packages, freed {:.2} MB", packages.len(), freed as f64 / 1048576.0);
    Ok(())
}

#[cfg(feature = "archlinux")]
fn query_foreign(_root: &Path, db_path: &Path, sync_dir: &Path) -> Result<()> {
    let db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
    let installed = db.list_installed(gen_id)?;

    let mut sync_names = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir(sync_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("db") {
                if let Ok(pkgs) = bulb::sync::SyncDb::parse_sync_db(&path) {
                    for pkg in &pkgs {
                        sync_names.insert(pkg.name.clone());
                    }
                }
            }
        }
    }

    let mut foreign: Vec<_> = installed.iter()
        .filter(|p| !sync_names.contains(&p.name))
        .collect();
    foreign.sort_by(|a, b| a.name.cmp(&b.name));

    if foreign.is_empty() {
        println!("no foreign packages found");
    } else {
        for pkg in &foreign {
            println!("{} {}", pkg.name, pkg.version);
        }
    }

    Ok(())
}

fn query_orphans(_root: &Path, db_path: &Path) -> Result<()> {
    let db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
    let installed = db.list_installed(gen_id)?;

    let mut dependents: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pkg in &installed {
        for dep in &pkg.depends {
            let dep_name = dep.to_string();
            *dependents.entry(dep_name).or_insert(0) += 1;
        }
    }

    let orphans: Vec<_> = installed.iter()
        .filter(|p| {
            let dep_count = dependents.get(&p.name).copied().unwrap_or(0);
            dep_count == 0 && p.name.contains("-")
        })
        .collect();

    if orphans.is_empty() {
        println!("no orphaned packages found");
    } else {
        for pkg in &orphans {
            println!("{} {}", pkg.name, pkg.version);
        }
    }

    Ok(())
}

#[cfg(feature = "archlinux")]
fn query_upgradable(_root: &Path, db_path: &Path, _store_path: &Path, sync_dir: &Path) -> Result<()> {
    let conf = bulb::config::pacman_conf::PacmanConf::load(std::path::Path::new("/etc/pacman.conf"))?;
    let db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
    let installed = db.list_installed(gen_id)?;

    let mut upgradable = Vec::new();

    for pkg in &installed {
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
                        upgradable.push((pkg.name.clone(), pkg.version.clone(), remote_version, repo.name.clone()));
                    }
                    break;
                }
            }
        }
    }

    upgradable.sort_by(|a, b| a.0.cmp(&b.0));

    if upgradable.is_empty() {
        println!("no packages to upgrade");
    } else {
        for (name, current, new, repo) in &upgradable {
            println!("{name}: {current} -> {new}  ({repo})");
        }
    }

    Ok(())
}

fn query_search(query: &str, _root: &Path, db_path: &Path) -> Result<()> {
    let db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(BulbError::NoCurrentGeneration)?;
    let installed = db.list_installed(gen_id)?;

    let query_lower = query.to_lowercase();
    let mut results: Vec<_> = installed.iter()
        .filter(|p| p.name.to_lowercase().contains(&query_lower)
            || p.description.as_deref().map_or(false, |d| d.to_lowercase().contains(&query_lower)))
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));

    for pkg in &results {
        println!("{} {}", pkg.name, pkg.version);
    }

    Ok(())
}

#[cfg(feature = "archlinux")]
fn query_sync_info(packages: &[String], sync_dir: &Path) -> Result<()> {
    for pkg_name in packages {
        let mut found = false;
        if let Ok(entries) = fs::read_dir(sync_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("db") {
                    if let Ok(pkgs) = bulb::sync::SyncDb::parse_sync_db(&path) {
                        for pkg in &pkgs {
                            if pkg.name == *pkg_name {
                                let repo = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                                println!("Repository     : {repo}");
                                println!("Name           : {}", pkg.name);
                                println!("Version        : {}", pkg.version);
                                println!("Architecture   : {}", pkg.arch);
                                if let Some(desc) = &pkg.description {
                                    println!("Description    : {desc}");
                                }
                                if !pkg.deps.is_empty() {
                                    println!("Depends On     : {}", pkg.deps.join(" "));
                                }
                                if !pkg.optdeps.is_empty() {
                                    println!("Optional Deps  : {}", pkg.optdeps.join(" "));
                                }
                                if !pkg.provides.is_empty() {
                                    println!("Provides       : {}", pkg.provides.join(" "));
                                }
                                if !pkg.conflicts.is_empty() {
                                    println!("Conflicts With : {}", pkg.conflicts.join(" "));
                                }
                                if let Some(filename) = &pkg.filename {
                                    println!("Filename       : {filename}");
                                }
                                if let Some(csize) = pkg.csize {
                                    println!("Compressed Size: {csize} bytes");
                                }
                                if let Some(isize) = pkg.isize {
                                    println!("Installed Size : {isize} bytes");
                                }
                                println!();
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        if !found {
            eprintln!("package {pkg_name} not found in any repository");
        }
    }
    Ok(())
}

#[cfg(feature = "archlinux")]
fn query_repo_list(repo: Option<&str>, sync_dir: &Path) -> Result<()> {
    let mut all_pkgs: Vec<(String, String, String)> = Vec::new();

    if let Ok(entries) = fs::read_dir(sync_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("db") {
                let repo_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                if let Some(filter) = repo {
                    if repo_name != filter {
                        continue;
                    }
                }
                if let Ok(pkgs) = bulb::sync::SyncDb::parse_sync_db(&path) {
                    for pkg in &pkgs {
                        all_pkgs.push((repo_name.to_string(), pkg.name.clone(), pkg.version.to_string()));
                    }
                }
            }
        }
    }

    all_pkgs.sort_by(|a, b| a.1.cmp(&b.1));

    if all_pkgs.is_empty() {
        println!("no packages found");
    } else {
        for (repo, name, version) in &all_pkgs {
            println!("{repo} {name} {version}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "archlinux")]
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
            noconfirm: false,
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
            noconfirm: false,
            command: Some(Commands::Install {
                targets: vec![package.to_string_lossy().into_owned()],
                force: false,
                needed: false,
                download_only: false,
            }),
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
            noconfirm: false,
            command: Some(Commands::Query {
                package: Some("hello".into()),
                info: true,
                list: false,
                owner: None,
                foreign: false,
                unneeded: false,
                upgradable: false,
                search: None,
                reasons: false,
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
            noconfirm: false,
            command: Some(Commands::Remove {
                package: "hello".into(),
                recursive: false,
                nosave: false,
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
    #[cfg(feature = "archlinux")]
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
            noconfirm: false,
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
            noconfirm: false,
            command: Some(Commands::Install {
                targets: vec![package.to_string_lossy().into_owned()],
                force: false,
                needed: false,
                download_only: false,
            }),
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
            noconfirm: false,
            command: Some(Commands::Rollback),
            query: None,
        };
        run(cli).unwrap();
        assert!(!root.join("usr/bin/testpkg").exists());
    }
}
