use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::core::pkginfo::PackageInfo;

pub struct DetailPanel;

impl DetailPanel {
    pub fn render(f: &mut Frame, area: Rect, pkg: &PackageInfo, scroll: u16) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name:        ", Style::default().fg(Color::DarkGray)),
                Span::styled(&pkg.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Version:     ", Style::default().fg(Color::DarkGray)),
                Span::raw(&pkg.version),
            ]),
            Line::from(vec![
                Span::styled("Arch:        ", Style::default().fg(Color::DarkGray)),
                Span::raw(&pkg.arch),
            ]),
        ];

        if let Some(ref desc) = pkg.description {
            lines.push(Line::from(vec![
                Span::styled("Description: ", Style::default().fg(Color::DarkGray)),
                Span::raw(desc.as_str()),
            ]));
        }

        if let Some(ref url) = pkg.url {
            lines.push(Line::from(vec![
                Span::styled("URL:         ", Style::default().fg(Color::DarkGray)),
                Span::styled(url.as_str(), Style::default().fg(Color::Blue)),
            ]));
        }

        if let Some(ref packager) = pkg.packager {
            lines.push(Line::from(vec![
                Span::styled("Packager:    ", Style::default().fg(Color::DarkGray)),
                Span::raw(packager.as_str()),
            ]));
        }

        if !pkg.depends.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Dependencies:",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
            for dep in &pkg.depends {
                lines.push(Line::from(Span::styled(
                    format!("  {dep}"),
                    Style::default().fg(Color::White),
                )));
            }
        }

        if !pkg.optdepends.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Optional:",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
            for dep in &pkg.optdepends {
                lines.push(Line::from(Span::styled(
                    format!("  {dep}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", pkg.name))
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }
}

pub struct StatusBar;

impl StatusBar {
    pub fn render(f: &mut Frame, area: Rect, status: &str, progress: &Option<(String, u32)>) {
        if let Some((msg, pct)) = progress {
            let bar_width = area.width.saturating_sub(msg.len() as u16 + 10) as usize;
            let filled = (bar_width as f64 * (*pct as f64 / 100.0)) as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar = format!("[{}{}] {}% {}", 
                "█".repeat(filled), 
                "░".repeat(empty), 
                pct, 
                msg
            );
            let status = Paragraph::new(Line::from(Span::styled(
                bar,
                Style::default().fg(Color::Green),
            )));
            f.render_widget(status, area);
        } else {
            let status = Paragraph::new(Line::from(Span::styled(
                status,
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(status, area);
        }
    }
}

pub struct PackageList;

impl PackageList {
    pub fn render(
        f: &mut Frame,
        area: Rect,
        packages: &[PackageInfo],
        selected: Option<usize>,
        scroll: u16,
    ) {
        let items: Vec<Line> = packages
            .iter()
            .enumerate()
            .skip(scroll as usize)
            .map(|(i, pkg)| {
                let style = if Some(i) == selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::styled(
                        format!("{:<30}", pkg.name),
                        style,
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<20}", pkg.version),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Packages ({}) ", packages.len()));

        let paragraph = Paragraph::new(items).block(block).scroll((0, 0));
        f.render_widget(paragraph, area);
    }
}

pub struct SearchBar;

impl SearchBar {
    pub fn render(f: &mut Frame, area: Rect, query: &str, active: bool) {
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let prefix = if active { "/" } else { ">" };
        let line = Line::from(vec![
            Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(query, Style::default().fg(Color::White)),
            if active {
                Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK))
            } else {
                Span::raw("")
            },
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Search")
            .border_style(style);

        let paragraph = Paragraph::new(line).block(block);
        f.render_widget(paragraph, area);
    }
}

pub struct ProgressBar;

impl ProgressBar {
    pub fn render(f: &mut Frame, area: Rect, progress: u32, label: &str) {
        let bar_width = area.width.saturating_sub(10) as usize;
        let filled = (bar_width as f64 * (progress as f64 / 100.0)) as usize;
        let empty = bar_width.saturating_sub(filled);

        let bar = format!("{} {}%", 
            format!("[{}{}]", "█".repeat(filled), "░".repeat(empty)),
            progress
        );

        let line = Line::from(vec![
            Span::styled(bar, Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(label, Style::default().fg(Color::DarkGray)),
        ]);

        let paragraph = Paragraph::new(line);
        f.render_widget(paragraph, area);
    }
}
