use std::env;
use std::io;
use std::io::Write;

use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};

use anyhow::Result;

/// UI layer for MCX.
///
/// - Uses ratatui + crossterm when running in an interactive terminal.
/// - Falls back to plain stdout/stderr for non-interactive runs.
/// - Tries to derive theme colors from wallpaper caches produced by wallust/pywal.
pub struct UserInterface;

#[derive(Clone, Debug)]
struct Theme {
    fg: Color,
    muted: Color,
    primary: Color,
    success: Color,
    warning: Color,
    error: Color,
    border: Color,
    panel_bg: Color,
}

impl Theme {
    fn default_dark() -> Self {
        Self {
            fg: Color::White,
            muted: Color::DarkGray,
            primary: Color::Cyan,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            border: Color::DarkGray,
            panel_bg: Color::Rgb(20, 20, 25),
        }
    }

    fn from_wallust_cache() -> Self {
        let mut t = Self::default_dark();
        if env::var("MCX_NO_WALLUST").ok().as_deref() == Some("1") {
            return t;
        }

        // Heuristic: look for cached palette JSON and extract hexes.
        // We intentionally avoid calling non-existing wallust APIs for runtime palette.
        let home = env::var("HOME").ok();
        if let Some(h) = home {
            let candidates = [
                format!("{}/.cache/wallust/palette.json", h),
                format!("{}/.cache/wallust/colors/palette.json", h),
                format!("{}/.cache/wal/colors.json", h),
            ];

            for cand in candidates {
                if let Ok(s) = std::fs::read_to_string(&cand) {
                    let hexes: Vec<&str> = s
                        .split(|c: char| !c.is_ascii_hexdigit())
                        .filter(|h| h.len() == 6 || h.len() == 3)
                        .take(8)
                        .collect();

                    if !hexes.is_empty() {
                        if let Some(c) = parse_hex_color(hexes[0]) {
                            t.primary = c;
                        }
                        if let Some(c) = hexes.get(1).and_then(|h| parse_hex_color(h)) {
                            t.panel_bg = c;
                        }
                        if let Some(c) = hexes.get(2).and_then(|h| parse_hex_color(h)) {
                            t.border = c;
                        }
                        if let Some(c) = hexes.get(3).and_then(|h| parse_hex_color(h)) {
                            t.success = c;
                        }
                        if let Some(c) = hexes.get(4).and_then(|h| parse_hex_color(h)) {
                            t.warning = c;
                        }
                        if let Some(c) = hexes.get(5).and_then(|h| parse_hex_color(h)) {
                            t.error = c;
                        }
                        return t;
                    }
                }
            }
        }

        t
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let h = hex.trim();
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else if h.len() == 3 {
        let r = u8::from_str_radix(&h[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&h[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&h[2..3].repeat(2), 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiMode {
    Tui,
    Fallback,
}

impl UiMode {
    fn detect() -> Self {
        if env::var("MCX_TUI").ok().as_deref() == Some("0") {
            return UiMode::Fallback;
        }
        let term = env::var("TERM").unwrap_or_default();
        if term == "dumb" {
            return UiMode::Fallback;
        }
        // best-effort: if stdin/stdout are piped, TUI will be ugly; keep fallback.
        // We can't depend on atty; so use a conservative heuristic.
        if env::var("CI").is_ok() {
            return UiMode::Fallback;
        }
        UiMode::Tui
    }
}

struct TuiSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    theme: Theme,
}

impl TuiSession {
    fn new() -> Result<Self> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        let theme = Theme::from_wallust_cache();
        Ok(Self { terminal, theme })
    }

    fn shutdown(mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }

    fn draw_frame<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Frame<'_>, &Theme),
    {
        self.terminal.draw(|frame| f(frame, &self.theme))?;
        Ok(())
    }
}

impl UserInterface {
    fn run_once(f: impl FnOnce(&mut TuiSession) -> Result<()>) -> Result<()> {
        let mode = UiMode::detect();
        if mode == UiMode::Fallback {
            return Ok(());
        }
        let mut session = TuiSession::new()?;
        let res = f(&mut session);
        session.shutdown();
        res
    }

    fn run_once_bool(f: impl FnOnce(&mut TuiSession) -> Result<bool>) -> bool {
        let mode = UiMode::detect();
        if mode == UiMode::Fallback {
            return false;
        }
        let mut session = match TuiSession::new() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let res = f(&mut session).unwrap_or(false);
        session.shutdown();
        res
    }

    pub fn display_info(message: &str) {
        if UiMode::detect() == UiMode::Fallback {
            println!(" [○] :: {}", message);
            return;
        }
        let _ = Self::run_once(|s| Self::toast(s, "INFO", message, s.theme.primary));
    }

    pub fn display_error(message: &str) {
        if UiMode::detect() == UiMode::Fallback {
            eprintln!(" [x] :: {}", message);
            return;
        }
        let _ = Self::run_once(|s| Self::toast(s, "ERROR", message, s.theme.error));
    }

    pub fn display_success(message: &str) {
        if UiMode::detect() == UiMode::Fallback {
            println!(" [√] :: {}", message);
            return;
        }
        let _ = Self::run_once(|s| Self::toast(s, "OK", message, s.theme.success));
    }

    pub fn display_warning(message: &str) {
        if UiMode::detect() == UiMode::Fallback {
            println!(" [!] :: {}", message);
            return;
        }
        let _ = Self::run_once(|s| Self::toast(s, "WARN", message, s.theme.warning));
    }

    pub fn display_progress(current: usize, total: usize, prefix: &str) {
        if UiMode::detect() == UiMode::Fallback {
            let percentage = if total > 0 { (current * 100) / total } else { 0 };
            let bar_width: usize = 10;
            let filled_blocks = if total > 0 { (current * bar_width) / total } else { 0 };
            let filled = "█".repeat(filled_blocks);
            let empty = "░".repeat(bar_width.saturating_sub(filled_blocks));
            print!(
                "\r   ⤷  {:<14} [{}{}] {:3}% ({}/{})",
                prefix, filled, empty, percentage, current, total
            );
            let _ = io::stdout().flush();
            return;
        }

        let _ = Self::run_once(|s| {
            let pct = if total == 0 {
                0
            } else {
                ((current as u64 * 100) / total as u64).min(100) as u16
            };

            s.draw_frame(|f, theme| {
                let area = f.area();
                let bar_w = area.width.saturating_sub(10).min(60);
                let filled = (bar_w as u32 * pct as u32 / 100) as usize;
                let empty = bar_w as usize - filled;
                let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

                let title = Span::styled(
                    " Progress ",
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                );

                let p = Paragraph::new(format!(
                    "{}\n\n  [{bar}] {}% ({}/{})\n\n  q: close",
                    prefix, pct, current, total
                ))
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border)),
                )
                .style(Style::default().fg(theme.fg));

                f.render_widget(Clear, area);
                f.render_widget(p, area);
            })?;

            // allow user to close
            loop {
                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(key) = event::read()? {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            break;
                        }
                    }
                }
            }

            Ok(())
        });
    }

    pub fn prompt_confirmation(prompt: &str) -> bool {
        if UiMode::detect() == UiMode::Fallback {
            print!("  ? {} [y/N] ❯ ", prompt);
            let _ = io::stdout().flush();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                return false;
            }
            let trimmed = input.trim().to_lowercase();
            return trimmed == "y" || trimmed == "yes";
        }

        Self::run_once_bool(|s| Self::confirm_dialog_inner(s, prompt))
    }

    pub fn render_list(title: &str, items: &[String]) {
        if UiMode::detect() == UiMode::Fallback {
            let width: usize = 50;
            let fill_len = width.saturating_sub(title.len() + 5);
            println!("\n  ┌── {} {}", title, "─".repeat(fill_len));
            if items.is_empty() {
                println!("  └─ (none)");
                return;
            }
            for (i, item) in items.iter().enumerate() {
                if i + 1 == items.len() {
                    println!("  └─ {}", item);
                } else {
                    println!("  ├─ {}", item);
                }
            }
            return;
        }

        let _ = Self::run_once(|s| Self::list_view(s, title, items));
    }

    pub fn render_table(title: &str, headers: &[&str], rows: &[Vec<String>]) {
        if UiMode::detect() == UiMode::Fallback {
            if headers.is_empty() {
                return;
            }
            // simple fallback: render as key-values-ish paragraph
            let mut lines = Vec::new();
            lines.push(format!("{}", title));
            lines.push(format!("{}", headers.join(" | ")));
            for r in rows {
                lines.push(r.join(" | "));
            }
            Self::render_block_message(title, &lines.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            return;
        }

        let _ = Self::run_once(|s| Self::table_view(s, title, headers, rows));
    }

    pub fn render_key_values(title: &str, pairs: &[(&str, &str)]) {
        if UiMode::detect() == UiMode::Fallback {
            if pairs.is_empty() {
                return;
            }
            let mut max_key_len = 0;
            for (k, _) in pairs {
                max_key_len = max_key_len.max(k.len());
            }
            let width: usize = 50;
            let fill_len = width.saturating_sub(title.len() + 5);
            println!("\n  ┌── {} {}", title, "─".repeat(fill_len));
            for (idx, (k, v)) in pairs.iter().enumerate() {
                if idx + 1 == pairs.len() {
                    println!("  └── {:<width$} : {}", k, v, width = max_key_len);
                } else {
                    println!("  ├── {:<width$} : {}", k, v, width = max_key_len);
                }
            }
            return;
        }

        let _ = Self::run_once(|s| {
            let lines = pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>();
            Self::paragraph_view(s, title, &lines)
        });
    }

    pub fn render_block_message(title: &str, lines: &[&str]) {
        if UiMode::detect() == UiMode::Fallback {
            let width: usize = 60;
            let fill_len = width.saturating_sub(title.len() + 5);
            println!("\n  ┌── {} {}", title, "─".repeat(fill_len));
            for line in lines {
                println!("  │ {}", line);
            }
            println!("  └──{}", "─".repeat(width - 3));
            return;
        }

        let _ = Self::run_once(|s| {
            let owned = lines.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
            Self::paragraph_view(s, title, &owned)
        });
    }

    pub fn render_section_separator() {
        if UiMode::detect() == UiMode::Fallback {
            println!("\n  ─{}", "─".repeat(70));
            return;
        }

        let _ = Self::run_once(|s| {
            s.draw_frame(|f, theme| {
                let area = f.area();
                let sep = Paragraph::new(Span::styled(
                    "─".repeat(70),
                    Style::default().fg(theme.border),
                ))
                .block(Block::default().borders(Borders::NONE));
                f.render_widget(Clear, area);
                // just draw near top-left
                f.render_widget(sep, Rect::new(area.x, area.y, area.width, 1));
            })?;
            Ok(())
        });
    }

    fn toast(s: &mut TuiSession, tag: &str, message: &str, accent: Color) -> Result<()> {
        s.draw_frame(|f, theme| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(area);

            let header = Paragraph::new(Span::styled(
                format!(" {tag} "),
                Style::default()
                    .fg(accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border)),
            );

            let msg = Paragraph::new(message)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border)),
                )
                .style(Style::default().fg(theme.fg));

            f.render_widget(Clear, area);
            f.render_widget(header, chunks[0]);
            f.render_widget(msg, chunks[1]);
        })?;

        // Wait briefly; also allow keypress to close early.
        let _ = event::poll(Duration::from_millis(700));
        if event::poll(Duration::from_millis(0))? {
            let _ = event::read();
        }
        Ok(())
    }

    fn confirm_dialog_inner(s: &mut TuiSession, prompt: &str) -> Result<bool> {
        let mut selected = 0usize; // 0 = No, 1 = Yes
        loop {
            s.draw_frame(|f, theme| {
                let area = f.area();
                let popup = centered_rect(60, 10, area);

                f.render_widget(Clear, area);

                let block = Block::default()
                    .title(Span::styled(
                        " Confirm ",
                        Style::default()
                            .fg(theme.primary)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border));

                f.render_widget(block, popup);

                let body = Paragraph::new(prompt)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(theme.fg));

                f.render_widget(body, Rect::new(popup.x + 2, popup.y + 2, popup.width - 4, popup.height - 5));

                let y = popup.y + popup.height - 3;
                let yes = if selected == 1 {
                    Style::default().fg(theme.fg).add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(theme.fg)
                };
                let no = if selected == 0 {
                    Style::default().fg(theme.fg).add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(theme.fg)
                };

                let line = Paragraph::new(Span::styled(
                    if selected == 0 {
                        " No     Yes "
                    } else {
                        " No     Yes "
                    },
                    Style::default().fg(theme.muted),
                ));

                // Render buttons as two separate paragraphs for style.
                let btn_no = Paragraph::new(Span::styled(" No ", no));
                let btn_yes = Paragraph::new(Span::styled(" Yes ", yes));

                f.render_widget(btn_no, Rect::new(popup.x + 6, y, 8, 1));
                f.render_widget(btn_yes, Rect::new(popup.x + popup.width - 14, y, 8, 1));
                f.render_widget(line, Rect::new(popup.x + 1, y, popup.width - 2, 1));
            })?;

            if event::poll(Duration::from_millis(60))? {
                if let Event::Key(key) = event::read()? {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Ok(false);
                    }
                    match key.code {
                        KeyCode::Left | KeyCode::Right => {
                            selected = if selected == 0 { 1 } else { 0 };
                        }
                        KeyCode::Char('y') | KeyCode::Enter => return Ok(selected == 1),
                        KeyCode::Esc | KeyCode::Char('n') => return Ok(false),
                        _ => {}
                    }
                }
            }
        }
    }

    fn list_view(s: &mut TuiSession, title: &str, items: &[String]) -> Result<()> {
        let mut selected = 0usize;

        loop {
            s.draw_frame(|f, theme| {
                let area = f.area();
                let popup = Rect::new(area.x + 2, area.y + 2, area.width - 4, area.height - 4);

                f.render_widget(Clear, area);

                let block = Block::default()
                    .title(Span::styled(
                        format!(" {title} "),
                        Style::default()
                            .fg(theme.primary)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border));

                let list_items: Vec<ListItem> = items
                    .iter()
                    .enumerate()
                    .map(|(i, it)| {
                        let style = if i == selected {
                            Style::default().fg(theme.fg).bg(theme.primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.fg)
                        };
                        ListItem::new(Span::styled(it.clone(), style))
                    })
                    .collect();

                let list = List::new(list_items)
                    .block(block)
                    .style(Style::default().fg(theme.fg));

                f.render_widget(list, popup);

                let hint = Paragraph::new("↑/↓ or j/k • Enter select • Esc/q close")
                    .style(Style::default().fg(theme.muted));
                f.render_widget(hint, Rect::new(popup.x + 1, popup.y + popup.height - 1, popup.width - 2, 1));
            })?;

            if event::poll(Duration::from_millis(60))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => {
                            selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if selected + 1 < items.len() {
                                selected += 1;
                            }
                        }
                        KeyCode::Enter => return Ok(()),
                        _ => {}
                    }
                }
            }
        }
    }

    fn table_view(s: &mut TuiSession, title: &str, headers: &[&str], rows: &[Vec<String>]) -> Result<()> {
        // ratatui table widget API differs by version; to keep this compile-stable,
        // render a paragraph view for now.
        let mut lines = Vec::new();
        lines.push(format!("{title}"));
        lines.push(headers.join(" | "));
        for r in rows {
            lines.push(r.join(" | "));
        }
        Self::paragraph_view(s, title, &lines)
    }

    fn paragraph_view(s: &mut TuiSession, title: &str, lines: &[String]) -> Result<()> {
        s.draw_frame(|f, theme| {
            let area = f.area();
            let popup = Rect::new(area.x + 2, area.y + 2, area.width - 4, area.height - 4);
            f.render_widget(Clear, area);
            let p = Paragraph::new(lines.join("\n"))
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(Span::styled(
                            format!(" {title} "),
                            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
                        ))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border)),
                )
                .style(Style::default().fg(theme.fg));
            f.render_widget(p, popup);
        })?;

        loop {
            if event::poll(Duration::from_millis(60))? {
                if let Event::Key(key) = event::read()? {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn centered_rect(percent_width: u16, percent_height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_height) / 2),
            Constraint::Percentage(percent_height),
            Constraint::Percentage((100 - percent_height) / 2),
        ])
        .split(r);

    let vertical = popup_layout[1];

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_width) / 2),
            Constraint::Percentage(percent_width),
            Constraint::Percentage((100 - percent_width) / 2),
        ])
        .split(vertical)[1]
}
