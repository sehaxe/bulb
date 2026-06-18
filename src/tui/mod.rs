pub mod search;
pub mod widgets;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};
use ratatui::Frame;
use ratatui::Terminal;

use crate::core::pkginfo::PackageInfo;
use crate::db::Database;
use crate::db::store::ContentStore;

use self::search::FuzzySearch;
use self::widgets::{DetailPanel, StatusBar};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Installed,
    Search,
    Generations,
}

impl Tab {
    fn all() -> &'static [Tab] {
        &[Tab::Installed, Tab::Search, Tab::Generations]
    }

    fn title(&self) -> &str {
        match self {
            Tab::Installed => "Installed",
            Tab::Search => "Search",
            Tab::Generations => "Generations",
        }
    }

    fn index(&self) -> usize {
        match self {
            Tab::Installed => 0,
            Tab::Search => 1,
            Tab::Generations => 2,
        }
    }

    fn from_index(i: usize) -> Self {
        match i {
            0 => Tab::Installed,
            1 => Tab::Search,
            _ => Tab::Generations,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    Searching,
    ConfirmInstall,
    ConfirmRemove,
}

struct GenEntry {
    id: i64,
    parent: Option<i64>,
    note: String,
    is_current: bool,
}

struct App {
    tab: Tab,
    input_mode: InputMode,
    search_input: String,
    installed: Vec<PackageInfo>,
    filtered: Vec<usize>,
    gen_entries: Vec<GenEntry>,
    list_state: ListState,
    detail_scroll: u16,
    show_detail: bool,
    status: String,
    progress: Option<(String, u32)>,
    fuzzy: FuzzySearch,
    needs_refresh: bool,
    selected_package: Option<String>,
}

impl App {
    fn new(db_path: &PathBuf, _store_path: &PathBuf) -> Self {
        let installed = load_installed(db_path);
        let gen_entries = load_generations(db_path);
        let fuzzy = FuzzySearch::new(&installed);

        let filtered: Vec<usize> = (0..installed.len()).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            tab: Tab::Installed,
            input_mode: InputMode::Normal,
            search_input: String::new(),
            installed,
            filtered,
            gen_entries,
            list_state,
            detail_scroll: 0,
            show_detail: false,
            status: String::from("q: quit  /: search  Tab: switch  Enter: details  i: install  r: remove"),
            progress: None,
            fuzzy,
            needs_refresh: false,
            selected_package: None,
        }
    }

    fn select_next(&mut self) {
        let i = self.list_state.selected().map(|i| i + 1).unwrap_or(0);
        if i < self.filtered.len() {
            self.list_state.select(Some(i));
            self.detail_scroll = 0;
        }
    }

    fn select_prev(&mut self) {
        let i = self.list_state.selected()
            .map(|i| if i > 0 { i - 1 } else { 0 })
            .unwrap_or(0);
        self.list_state.select(Some(i));
        self.detail_scroll = 0;
    }

    fn current_package(&self) -> Option<&PackageInfo> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .and_then(|&idx| self.installed.get(idx))
    }

    fn update_filter(&mut self) {
        if self.search_input.is_empty() {
            self.filtered = (0..self.installed.len()).collect();
        } else {
            self.filtered = self.fuzzy.search(&self.search_input);
        }
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
        self.detail_scroll = 0;
    }
}

fn load_installed(db_path: &std::path::Path) -> Vec<PackageInfo> {
    Database::open(db_path)
        .ok()
        .and_then(|db| {
            let cur_gen = db.current_generation().ok().flatten()?;
            db.list_installed(cur_gen).ok()
        })
        .unwrap_or_default()
}

fn load_generations(db_path: &std::path::Path) -> Vec<GenEntry> {
    Database::open(db_path)
        .ok()
        .and_then(|db| db.list_generations().ok())
        .map(|gens| {
            gens.into_iter()
                .map(|(id, parent, note, is_current)| GenEntry { id, parent, note, is_current })
                .collect()
        })
        .unwrap_or_default()
}

pub fn run_app(root: PathBuf, db_path: PathBuf, store_path: PathBuf) -> crate::error::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&db_path, &store_path);

    let tick_rate = Duration::from_millis(50);

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.input_mode {
                    InputMode::Normal => handle_normal_key(&mut app, key),
                    InputMode::Searching => handle_search_key(&mut app, key),
                    InputMode::ConfirmInstall => {
                        let pkg_name = app.selected_package.clone();
                        handle_confirm_action(&mut app, key, &pkg_name, true, &root, &db_path, &store_path);
                    }
                    InputMode::ConfirmRemove => {
                        let pkg_name = app.selected_package.clone();
                        handle_confirm_action(&mut app, key, &pkg_name, false, &root, &db_path, &store_path);
                    }
                }
            }
        }

        if app.needs_refresh {
            app.installed = load_installed(&db_path);
            app.gen_entries = load_generations(&db_path);
            app.fuzzy.update_packages(&app.installed);
            app.update_filter();
            app.needs_refresh = false;
        }

        if app.progress.is_some() {
            terminal.draw(|f| ui(f, &mut app))?;
        }
    }
}

fn handle_normal_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => {
            app.status = "quitting...".into();
        }
        KeyCode::Char('/') | KeyCode::Char('s') => {
            app.input_mode = InputMode::Searching;
            app.tab = Tab::Search;
            app.status = "type to search... ESC to cancel".into();
        }
        KeyCode::Tab => {
            let next = (app.tab.index() + 1) % Tab::all().len();
            app.tab = Tab::from_index(next);
            app.update_filter();
        }
        KeyCode::BackTab => {
            let prev = if app.tab.index() == 0 { Tab::all().len() - 1 } else { app.tab.index() - 1 };
            app.tab = Tab::from_index(prev);
            app.update_filter();
        }
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
        KeyCode::PageDown => {
            for _ in 0..10 { app.select_next(); }
        }
        KeyCode::PageUp => {
            for _ in 0..10 { app.select_prev(); }
        }
        KeyCode::Home | KeyCode::Char('g') => {
            app.list_state.select(if app.filtered.is_empty() { None } else { Some(0) });
            app.detail_scroll = 0;
        }
        KeyCode::End | KeyCode::Char('G') => {
            let last = if app.filtered.is_empty() { 0 } else { app.filtered.len() - 1 };
            app.list_state.select(Some(last));
            app.detail_scroll = 0;
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            app.show_detail = !app.show_detail;
        }
        KeyCode::Char('i') => {
            if let Some(pkg) = app.current_package() {
                let name = pkg.name.clone();
                app.selected_package = Some(name.clone());
                app.input_mode = InputMode::ConfirmInstall;
                app.status = format!("install {name}? [y/N]");
            }
        }
        KeyCode::Char('r') => {
            if let Some(pkg) = app.current_package() {
                let name = pkg.name.clone();
                app.selected_package = Some(name.clone());
                app.input_mode = InputMode::ConfirmRemove;
                app.status = format!("remove {name}? [y/N]");
            }
        }
        KeyCode::Char('1') => { app.tab = Tab::Installed; app.update_filter(); }
        KeyCode::Char('2') => { app.tab = Tab::Search; app.update_filter(); }
        KeyCode::Char('3') => { app.tab = Tab::Generations; }
        _ => {}
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.search_input.clear();
            app.update_filter();
            app.tab = Tab::Installed;
            app.status = "q: quit  /: search  Tab: switch  Enter: details".into();
        }
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            app.tab = Tab::Search;
            app.status = format!("{} results", app.filtered.len());
        }
        KeyCode::Backspace => {
            app.search_input.pop();
            app.update_filter();
        }
        KeyCode::Char(c) => {
            app.search_input.push(c);
            app.update_filter();
        }
        _ => {}
    }
}

fn handle_confirm_action(
    app: &mut App,
    key: KeyEvent,
    pkg_name: &Option<String>,
    is_install: bool,
    root: &PathBuf,
    db_path: &PathBuf,
    store_path: &PathBuf,
) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(name) = pkg_name {
                let name = name.clone();
                app.input_mode = InputMode::Normal;
                if is_install {
                    app.status = format!("installing {name}...");
                    match do_install(&name, root, db_path, store_path) {
                        Ok(msg) => {
                            app.status = msg;
                            app.needs_refresh = true;
                        }
                        Err(e) => {
                            app.status = format!("error: {e}");
                        }
                    }
                } else {
                    app.status = format!("removing {name}...");
                    match do_remove(&name, root, db_path) {
                        Ok(msg) => {
                            app.status = msg;
                            app.needs_refresh = true;
                        }
                        Err(e) => {
                            app.status = format!("error: {e}");
                        }
                    }
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            app.selected_package = None;
            app.status = "cancelled".into();
        }
        _ => {}
    }
}

fn do_install(name: &str, root: &PathBuf, db_path: &PathBuf, store_path: &PathBuf) -> crate::error::Result<String> {
    let cache_dir = store_path.parent().unwrap_or(store_path).join("cache");
    let entries: Vec<_> = std::fs::read_dir(&cache_dir)
        .map_err(|e| crate::error::BulbError::Config(format!("cache read: {e}")))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with(name) && n.ends_with(".pkg.tar.zst")
        })
        .collect();

    let pkg = entries.first()
        .ok_or_else(|| crate::error::BulbError::PackageNotFound(format!("{name} not in cache")))?;

    let pkg_path = pkg.path();
    let pkg_name = pkg_path.file_name().unwrap().to_string_lossy().into_owned();

    let mut db = Database::open(db_path)?;
    let _gen_id = db.ensure_generation()?;
    let store = ContentStore::new(store_path.clone());
    store.init()?;

    let file_name = &pkg_name;
    let (info, extracted_files) = if file_name.ends_with(".pkg.tar.zst") {
        let file = std::fs::File::open(&pkg_path)?;
        let buf_reader = std::io::BufReader::with_capacity(1024 * 1024, file);
        let decoder = zstd::stream::Decoder::with_buffer(buf_reader)?;
        let mut archive = tar::Archive::new(decoder);
        extract_from_archive(&mut archive, root, &store)?
    } else {
        return Err(crate::error::BulbError::UnsupportedPackageFormat(pkg_path));
    };

    let new_gen = db.create_generation(&format!("install {name}"))?;
    db.insert_installed_package(new_gen, &info, &extracted_files, &format!("installed-{}", info.name))?;

    Ok(format!("installed {} {}", info.name, info.version))
}

fn extract_from_archive<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    root: &PathBuf,
    store: &ContentStore,
) -> crate::error::Result<(PackageInfo, Vec<PathBuf>)> {
    use std::io::Read as _;

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
                            let _ = std::fs::create_dir(&dest);
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
                                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode));
                            }
                        }
                    }
                    tar::EntryType::Symlink => {
                        ensure_parent_dir(&dest, root, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let _ = std::fs::remove_file(&dest);
                            #[cfg(unix)]
                            std::os::unix::fs::symlink(&link_target, &dest)?;
                        }
                    }
                    tar::EntryType::Link => {
                        ensure_parent_dir(&dest, root, &mut created_dirs)?;
                        if let Some(link_target) = entry.link_name()? {
                            let link_dest = root.join(&link_target);
                            let _ = std::fs::remove_file(&dest);
                            std::fs::hard_link(&link_dest, &dest)?;
                        }
                    }
                    _ => continue,
                }
                files.push(relative);
            }
        }
    }

    let pkginfo_text = pkginfo_text.ok_or_else(|| {
        crate::error::BulbError::InvalidMetadata("archive missing .PKGINFO".into())
    })?;
    let pkginfo = crate::format::alpm::pkginfo::PkgInfo::parse(&pkginfo_text);
    let info = crate::format::alpm::convert::package_info_from_pkginfo(&pkginfo);

    Ok((info, files))
}

fn normalize_path(path: &std::path::Path) -> crate::error::Result<PathBuf> {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(crate::error::BulbError::UnsafeArchivePath(path.display().to_string()));
            }
        }
    }
    Ok(normalized)
}

fn ensure_parent_dir(
    path: &std::path::Path,
    root: &std::path::Path,
    created_dirs: &mut std::collections::HashSet<PathBuf>,
) -> crate::error::Result<()> {
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
                    let _ = std::fs::create_dir(&dir);
                }
            }
        }
    }
    Ok(())
}

fn do_remove(name: &str, root: &PathBuf, db_path: &PathBuf) -> crate::error::Result<String> {
    let mut db = Database::open(db_path)?;
    let gen_id = db.current_generation()?.ok_or(crate::error::BulbError::NoCurrentGeneration)?;

    let files = db.get_installed_files(gen_id, name)?;
    let info = db.get_installed_package(gen_id, name)?
        .ok_or_else(|| crate::error::BulbError::PackageNotFound(name.into()))?;

    let new_gen = db.create_generation(&format!("remove {name}"))?;
    db.remove_package(new_gen, name)?;

    for file in files.iter().rev() {
        let path = root.join(file);
        if path.is_file() || std::fs::symlink_metadata(&path).is_ok() {
            std::fs::remove_file(&path)?;
        } else if path.is_dir() {
            let _ = std::fs::remove_dir(&path);
        }
    }

    Ok(format!("removed {} {}", info.name, info.version))
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);

    let body_chunks = if app.show_detail {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(chunks[1])
    };

    match app.tab {
        Tab::Installed | Tab::Search => {
            render_package_list(f, app, body_chunks[0]);
            if app.show_detail {
                if let Some(detail_area) = body_chunks.get(1) {
                    render_detail(f, app, *detail_area);
                }
            }
        }
        Tab::Generations => {
            render_generations(f, app, body_chunks[0]);
        }
    }

    StatusBar::render(f, chunks[2], &app.status, &app.progress);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::all()
        .iter()
        .map(|t| Line::from(Span::styled(
            t.title(),
            if *t == app.tab {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )))
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("bulb"))
        .select(app.tab.index())
        .style(Style::default().fg(Color::White));

    f.render_widget(tabs, area);
}

fn render_package_list(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.tab {
        Tab::Installed => format!("Installed ({})", app.filtered.len()),
        Tab::Search => {
            if app.search_input.is_empty() {
                format!("Search ({})", app.installed.len())
            } else {
                format!("Search: {} ({} results)", app.search_input, app.filtered.len())
            }
        }
        _ => String::new(),
    };

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&idx| {
            let pkg = &app.installed[idx];
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<30}", pkg.name),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<20}", pkg.version),
                    Style::default().fg(Color::White),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<10}", pkg.arch),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    if let Some(pkg) = app.current_package() {
        DetailPanel::render(f, area, pkg, app.detail_scroll);
    } else {
        let empty = Paragraph::new("No package selected")
            .block(Block::default().borders(Borders::ALL).title("Details"));
        f.render_widget(empty, area);
    }
}

fn render_generations(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .gen_entries
        .iter()
        .map(|entry| {
            let marker = if entry.is_current { " *" } else { "" };
            let parent_str = entry.parent.map(|p| p.to_string()).unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>6}", entry.id),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("parent={:>6}", parent_str),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{}{}", entry.note, marker),
                    if entry.is_current {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!("Generations ({})", app.gen_entries.len())))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !app.gen_entries.is_empty() {
        state.select(Some(0));
    }
    f.render_stateful_widget(list, area, &mut state);
}
