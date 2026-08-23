use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use anyhow::{Result, Context};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    style::{Color, SetBackgroundColor, SetForegroundColor, ResetColor, Print},
};

pub enum ConfigTarget {
    EngineConfig,
    RepoConfig,
}

/// Greatest char boundary in `s` at or before byte index `i`.
fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Smallest char boundary in `s` at or after byte index `i`.
fn ceil_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Greatest char boundary in `s` strictly before byte index `i`
/// (the start of the character preceding `i`).
fn prev_char_boundary(s: &str, i: usize) -> usize {
    floor_char_boundary(s, i.saturating_sub(1))
}

pub struct ConfigEditorCommand {
    config_path: PathBuf,
}

impl ConfigEditorCommand {
    pub fn new(root: &str, target: ConfigTarget) -> Self {
        let filename = match target {
            ConfigTarget::EngineConfig => "config.ini",
            ConfigTarget::RepoConfig => "repo.ini",
        };
        Self {
            config_path: PathBuf::from(root).join(format!("etc/mcx/{}", filename)),
        }
    }

    pub fn execute(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = if self.config_path.exists() {
            fs::read_to_string(&self.config_path)
                .with_context(|| format!("Failed to read configuration: {:?}", self.config_path))?
        } else {
            String::new()
        };

        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        if lines.is_empty() { lines.push(String::new()); }

        let mut stdout = io::stdout();
        terminal::enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen, cursor::Show)?;

        let mut cursor_x = 0;
        let mut cursor_y = 0;
        let mut scroll_y = 0;
        let mut scroll_x = 0;

        let mut is_dirty = false;
        let mut cut_buffer: Option<String> = None;
        let mut status_message = format!("Opened: {:?}", self.config_path);
        let mut quit_confirm = false;

        loop {
            let (terminal_width, terminal_height) = terminal::size()?;
            let text_height = (terminal_height as usize).saturating_sub(4);
            let text_width = terminal_width as usize;

            if cursor_y < scroll_y {
                scroll_y = cursor_y;
            } else if cursor_y >= scroll_y + text_height {
                scroll_y = cursor_y - text_height + 1;
            }

            if cursor_x < scroll_x {
                scroll_x = cursor_x;
            } else if cursor_x >= scroll_x + text_width {
                scroll_x = cursor_x - text_width + 1;
            }

            execute!(stdout, cursor::Hide, cursor::MoveTo(0, 0))?;

            execute!(stdout, SetBackgroundColor(Color::Black), SetForegroundColor(Color::White))?;
            let modified_tag = if is_dirty { " *MODIFIED* " } else { " " };
            let header_text = format!(" MCX Configuration Editor | MCX v7.0.0 |{}{:?}", modified_tag, self.config_path);
            execute!(stdout, Print(format!("{:width$}\r\n", header_text, width = text_width)), ResetColor)?;

            for i in 0..text_height {
                let file_y = scroll_y + i;
                execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;

                if file_y < lines.len() {
                    let line = &lines[file_y];
                    if line.len() > scroll_x {
                        let start = scroll_x;
                        let end = (scroll_x + text_width).min(line.len());
                        print!("{}", &line[start..end]);
                    }
                }
                print!("\r\n");
            }

            let status_bg = if is_dirty { Color::Grey } else { Color::DarkGrey };
            execute!(stdout, SetBackgroundColor(status_bg), SetForegroundColor(Color::White))?;

            let position_indicator = format!("Ln {}, Col {}", cursor_y + 1, cursor_x + 1);
            let free_space = text_width.saturating_sub(status_message.len() + position_indicator.len() + 2);
            let status_bar = format!(" {}{:free_space$}{} ", status_message, "", position_indicator);
            execute!(stdout, Print(format!("{}\r\n", status_bar)), ResetColor)?;

            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine), SetForegroundColor(Color::Cyan))?;
            if quit_confirm {
                print!(" Unsaved changes! Quit anyway? (y/N)\r\n");
            } else {
                print!(" Ctrl+X: Quit  |  Ctrl+S/Ctrl+O: Save  |  Ctrl+K: Cut Line  |  Ctrl+U: Paste Line\r\n");
            }
            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
            print!(" Ctrl+Y: Prev Line  |  Ctrl+V: Next Line  |  Tab: Insert 4 spaces  |  Ctrl+C: Cancel\r\n");
            execute!(stdout, ResetColor)?;

            let screen_x = (cursor_x - scroll_x) as u16;
            let screen_y = (cursor_y - scroll_y + 1) as u16;
            execute!(stdout, cursor::MoveTo(screen_x, screen_y), cursor::Show)?;
            stdout.flush()?;

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key_event) => {
                        if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                            match key_event.code {
                                KeyCode::Char('x') => {
                                    if is_dirty {
                                        quit_confirm = true;
                                        status_message = "Unsaved changes! Quit anyway? (y/N)".to_string();
                                        continue;
                                    }
                                    return self.exit_editor();
                                }
                                KeyCode::Char('s') | KeyCode::Char('o') => {
                                    self.save_file(&lines)?;
                                    is_dirty = false;
                                    status_message = "File saved successfully.".to_string();
                                    quit_confirm = false;
                                }
                                KeyCode::Char('k') => {
                                    if lines.len() > 1 {
                                        cut_buffer = Some(lines.remove(cursor_y));
                                        if cursor_y >= lines.len() { cursor_y = lines.len() - 1; }
                                        cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x.min(lines[cursor_y].len()));
                                    } else {
                                        cut_buffer = Some(lines[0].clone());
                                        lines[0].clear();
                                        cursor_x = 0;
                                    }
                                    is_dirty = true;
                                    status_message = "Line cut to clipboard.".to_string();
                                    quit_confirm = false;
                                }
                                KeyCode::Char('u') => {
                                    if let Some(ref buffer) = cut_buffer {
                                        lines.insert(cursor_y, buffer.clone());
                                        cursor_y += 1;
                                        is_dirty = true;
                                        status_message = "Line pasted from clipboard.".to_string();
                                    }
                                    quit_confirm = false;
                                }
                                KeyCode::Char('y') => {
                                    if cursor_y > 0 {
                                        cursor_y -= 1;
                                        cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x.min(lines[cursor_y].len()));
                                    }
                                    quit_confirm = false;
                                }
                                KeyCode::Char('v') => {
                                    if cursor_y + 1 < lines.len() {
                                        cursor_y += 1;
                                        cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x.min(lines[cursor_y].len()));
                                    }
                                    quit_confirm = false;
                                }
                                KeyCode::Char('c') => {
                                    return self.exit_editor();
                                }
                                _ => {
                                    quit_confirm = false;
                                }
                            }
                            continue;
                        }

                        if quit_confirm {
                            match key_event.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => return self.exit_editor(),
                                KeyCode::Char('n') | KeyCode::Char('N') => {
                                    quit_confirm = false;
                                    status_message = "Return to editor.".to_string();
                                    continue;
                                }
                                _ => { continue; }
                            }
                        }

                        match key_event.code {
                            KeyCode::Up => if cursor_y > 0 { cursor_y -= 1; cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x.min(lines[cursor_y].len())); },
                            KeyCode::Down => if cursor_y + 1 < lines.len() { cursor_y += 1; cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x.min(lines[cursor_y].len())); },
                            KeyCode::Left => {
                                if cursor_x > 0 {
                                    cursor_x = prev_char_boundary(&lines[cursor_y], cursor_x);
                                } else if cursor_y > 0 {
                                    cursor_y -= 1;
                                    cursor_x = lines[cursor_y].len();
                                }
                            }
                            KeyCode::Right => {
                                if cursor_x < lines[cursor_y].len() {
                                    cursor_x = ceil_char_boundary(&lines[cursor_y], cursor_x + 1);
                                } else if cursor_y + 1 < lines.len() {
                                    cursor_y += 1;
                                    cursor_x = 0;
                                }
                            }
                            KeyCode::PageUp => cursor_y = cursor_y.saturating_sub(text_height),
                            KeyCode::PageDown => cursor_y = (cursor_y + text_height).min(lines.len().saturating_sub(1)),
                            KeyCode::Home => cursor_x = 0,
                            KeyCode::End => cursor_x = lines[cursor_y].len(),
                            KeyCode::Char(c) => {
                                cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x);
                                lines[cursor_y].insert(cursor_x, c);
                                cursor_x += c.len_utf8();
                                is_dirty = true;
                                quit_confirm = false;
                            }
                            KeyCode::Tab => {
                                cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x);
                                lines[cursor_y].insert_str(cursor_x, "    ");
                                cursor_x += 4;
                                is_dirty = true;
                                quit_confirm = false;
                            }
                            KeyCode::Backspace => {
                                if cursor_x > 0 {
                                    let rm = prev_char_boundary(&lines[cursor_y], cursor_x);
                                    lines[cursor_y].remove(rm);
                                    cursor_x = rm;
                                    is_dirty = true;
                                } else if cursor_y > 0 {
                                    let current_line = lines.remove(cursor_y);
                                    cursor_y -= 1;
                                    cursor_x = lines[cursor_y].len();
                                    lines[cursor_y].push_str(&current_line);
                                    is_dirty = true;
                                }
                                quit_confirm = false;
                            }
                            KeyCode::Delete => {
                                if cursor_x < lines[cursor_y].len() {
                                    let idx = ceil_char_boundary(&lines[cursor_y], cursor_x);
                                    lines[cursor_y].remove(idx);
                                    is_dirty = true;
                                } else if cursor_y + 1 < lines.len() {
                                    let next_line = lines.remove(cursor_y + 1);
                                    lines[cursor_y].push_str(&next_line);
                                    is_dirty = true;
                                }
                                quit_confirm = false;
                            }
                            KeyCode::Enter => {
                                cursor_x = floor_char_boundary(&lines[cursor_y], cursor_x);
                                let current_line = &lines[cursor_y];
                                let next_line = current_line[cursor_x..].to_string();
                                lines[cursor_y] = current_line[..cursor_x].to_string();
                                lines.insert(cursor_y + 1, next_line);
                                cursor_y += 1;
                                cursor_x = 0;
                                is_dirty = true;
                                quit_confirm = false;
                            }
                            _ => { quit_confirm = false; }
                        }
                    }
                    Event::Resize(_, _) => {
                        quit_confirm = false;
                    }
                    Event::FocusGained | Event::FocusLost | Event::Paste(_) => {
                        quit_confirm = false;
                    }
                    _ => { quit_confirm = false; }
                }
            } else {
                // Timeout - just continue the loop (allows periodic re-render)
            }
        }
    }

    fn save_file(&self, lines: &[String]) -> Result<()> {
        let mut payload = lines.join("\n");
        if !payload.ends_with('\n') { payload.push('\n'); }
        fs::write(&self.config_path, payload)?;
        Ok(())
    }

    fn exit_editor(&self) -> Result<()> {
        let mut stdout = io::stdout();
        execute!(stdout, LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}