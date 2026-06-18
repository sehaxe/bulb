use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;

use crate::error::Result;

#[derive(Clone)]
pub struct SearchResult {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub selected: bool,
}

pub fn run_multi_select(
    results: Vec<SearchResult>,
    on_confirm: impl FnOnce(Vec<String>) -> Result<()>,
) -> Result<()> {
    if results.is_empty() {
        println!("No results found");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = MultiSelectApp::new(results);
    let tick_rate = Duration::from_millis(50);

    let result = loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        break Ok(());
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Char(' ') => app.toggle_selected(),
                    KeyCode::Enter => {
                        let selected: Vec<String> = app.results.iter()
                            .filter(|r| r.selected)
                            .map(|r| r.name.clone())
                            .collect();

                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                        if selected.is_empty() {
                            println!("No packages selected");
                            break Ok(());
                        }

                        println!("Installing {} packages...", selected.len());
                        for name in &selected {
                            println!("  - {name}");
                        }

                        break on_confirm(selected);
                    }
                    KeyCode::Char('a') => app.select_all(),
                    KeyCode::Char('n') => app.deselect_all(),
                    _ => {}
                }
            }
        }
    };

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    result
}

struct MultiSelectApp {
    results: Vec<SearchResult>,
    list_state: ListState,
}

impl MultiSelectApp {
    fn new(results: Vec<SearchResult>) -> Self {
        let mut list_state = ListState::default();
        if !results.is_empty() {
            list_state.select(Some(0));
        }
        Self { results, list_state }
    }

    fn select_next(&mut self) {
        let i = self.list_state.selected().map(|i| i + 1).unwrap_or(0);
        if i < self.results.len() {
            self.list_state.select(Some(i));
        }
    }

    fn select_prev(&mut self) {
        let i = self.list_state.selected()
            .map(|i| if i > 0 { i - 1 } else { 0 })
            .unwrap_or(0);
        self.list_state.select(Some(i));
    }

    fn toggle_selected(&mut self) {
        if let Some(i) = self.list_state.selected() {
            self.results[i].selected = !self.results[i].selected;
        }
    }

    fn select_all(&mut self) {
        for r in &mut self.results {
            r.selected = true;
        }
    }

    fn deselect_all(&mut self) {
        for r in &mut self.results {
            r.selected = false;
        }
    }
}

fn ui(f: &mut Frame, app: &mut MultiSelectApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    let selected_count = app.results.iter().filter(|r| r.selected).count();
    let header = Paragraph::new(Line::from(vec![
        Span::styled("bulb", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(
            format!("{} packages found", app.results.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Package Search"));
    f.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = app.results.iter().map(|r| {
        let marker = if r.selected { "●" } else { "○" };
        let marker_color = if r.selected { Color::Green } else { Color::DarkGray };

        ListItem::new(Line::from(vec![
            Span::styled(
                format!("{marker} "),
                Style::default().fg(marker_color),
            ),
            Span::styled(
                format!("{:<30}", r.name),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:<20}", r.version),
                Style::default().fg(Color::White),
            ),
            Span::raw(" "),
            Span::styled(
                format!("[{}]", r.repo),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Results"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    let footer_text = if selected_count > 0 {
        format!(
            "j/k: navigate  space: select  a: select all  n: deselect all  enter: install {selected_count} packages  q: quit"
        )
    } else {
        "j/k: navigate  space: select  a: select all  n: deselect all  q: quit".to_string()
    };

    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[2]);
}
