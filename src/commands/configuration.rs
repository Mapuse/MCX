use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
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
            let header_text = format!(" MCX Configuration Editor | MCX v2.7.8 |{}{:?}", modified_tag, self.config_path);
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
            print!(" Ctrl+X: Close Editor  |  Ctrl+O: Save File  |  Ctrl+K: Cut Line  |  Ctrl+Y: Move Up\r\n");
            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
            print!(" Ctrl+C: Drop Actions  |  Ctrl+S: Save  |  Ctrl+U: Paste Buffer  |  Ctrl+V: Move Down\r\n");
            execute!(stdout, ResetColor)?;

            let screen_x = (cursor_x - scroll_x) as u16;
            let screen_y = (cursor_y - scroll_y + 1) as u16;
            execute!(stdout, cursor::MoveTo(screen_x, screen_y), cursor::Show)?;
            stdout.flush()?;

            if let Event::Key(key_event) = event::read()? {
                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    match key_event.code {
                        KeyCode::Char('x') => {
                            if is_dirty {
                                status_message = "Unsaved edits! Quit anyway? (y/N)".to_string();
                                continue;
                            }
                            return self.exit_editor();
                        }
                        KeyCode::Char('o') | KeyCode::Char('s') => {
                            self.save_file(&lines)?;
                            is_dirty = false;
                            status_message = format!("File saved successfully.");
                        }
                        KeyCode::Char('k') => {
                            if lines.len() > 1 {
                                cut_buffer = Some(lines.remove(cursor_y));
                                if cursor_y >= lines.len() { cursor_y = lines.len() - 1; }
                                cursor_x = cursor_x.min(lines[cursor_y].len());
                            } else {
                                cut_buffer = Some(lines[0].clone());
                                lines[0].clear();
                                cursor_x = 0;
                            }
                            is_dirty = true;
                            status_message = "Line cut to clipboard.".to_string();
                        }
                        KeyCode::Char('u') => {
                            if let Some(ref buffer) = cut_buffer {
                                lines.insert(cursor_y, buffer.clone());
                                cursor_y += 1;
                                is_dirty = true;
                                status_message = "Line pasted from clipboard.".to_string();
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                if status_message.starts_with("Unsaved edits!") {
                    match key_event.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => return self.exit_editor(),
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            status_message = format!("Return to editor.");
                            continue;
                        }
                        _ => {}
                    }
                }

                match key_event.code {
                    KeyCode::Up => if cursor_y > 0 { cursor_y -= 1; cursor_x = cursor_x.min(lines[cursor_y].len()); },
                    KeyCode::Down => if cursor_y + 1 < lines.len() { cursor_y += 1; cursor_x = cursor_x.min(lines[cursor_y].len()); },
                    KeyCode::Left => {
                        if cursor_x > 0 { cursor_x -= 1; }
                        else if cursor_y > 0 { cursor_y -= 1; cursor_x = lines[cursor_y].len(); }
                    }
                    KeyCode::Right => {
                        if cursor_x < lines[cursor_y].len() { cursor_x += 1; }
                        else if cursor_y + 1 < lines.len() { cursor_y += 1; cursor_x = 0; }
                    }
                    KeyCode::PageUp => cursor_y = cursor_y.saturating_sub(text_height),
                    KeyCode::PageDown => cursor_y = (cursor_y + text_height).min(lines.len().saturating_sub(1)),
                    KeyCode::Home => cursor_x = 0,
                    KeyCode::End => cursor_x = lines[cursor_y].len(),
                    KeyCode::Char(c) => {
                        lines[cursor_y].insert(cursor_x, c);
                        cursor_x += 1;
                        is_dirty = true;
                    }
                    KeyCode::Backspace => {
                        if cursor_x > 0 {
                            lines[cursor_y].remove(cursor_x - 1);
                            cursor_x -= 1;
                            is_dirty = true;
                        } else if cursor_y > 0 {
                            let current_line = lines.remove(cursor_y);
                            cursor_y -= 1;
                            cursor_x = lines[cursor_y].len();
                            lines[cursor_y].push_str(&current_line);
                            is_dirty = true;
                        }
                    }
                    KeyCode::Delete => {
                        if cursor_x < lines[cursor_y].len() {
                            lines[cursor_y].remove(cursor_x);
                            is_dirty = true;
                        } else if cursor_y + 1 < lines.len() {
                            let next_line = lines.remove(cursor_y + 1);
                            lines[cursor_y].push_str(&next_line);
                            is_dirty = true;
                        }
                    }
                    KeyCode::Enter => {
                        let current_line = &lines[cursor_y];
                        let next_line = current_line[cursor_x..].to_string();
                        lines[cursor_y] = current_line[..cursor_x].to_string();
                        lines.insert(cursor_y + 1, next_line);
                        cursor_y += 1;
                        cursor_x = 0;
                        is_dirty = true;
                    }
                    _ => {}
                }
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
