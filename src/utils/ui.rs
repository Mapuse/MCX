use std::io::{self, Write};

pub struct UserInterface;

impl UserInterface {
    pub fn info(message: &str) {
        println!(" [○] :: {}", message);
    }

    pub fn error(message: &str) {
        eprintln!(" [x] :: {}", message);
    }

    pub fn success(message: &str) {
        println!(" [√] :: {}", message);
    }

    pub fn warning(message: &str) {
        println!(" [!] :: {}", message);
    }

    pub fn version(message: &str) {
        println!(" {}", message);
    }

    pub fn download(message: &str) {
        println!(" [↓] :: {}", message);
    }

    pub fn cas(message: &str) {
        println!(" [♻] :: {}", message);
    }

    pub fn sandbox(message: &str) {
        println!(" [▣] :: {}", message);
    }

    pub fn cgroup(message: &str) {
        println!(" [@] :: {}", message);
    }

    pub fn security(message: &str) {
        println!(" [$] :: {}", message);
    }

    pub fn profile(message: &str) {
        println!(" [⚙] :: {}", message);
    }

    pub fn self_update(message: &str) {
        println!(" [↑] :: {}", message);
    }

    pub fn progress(current: usize, total: usize, prefix: &str) {
        let percentage = if total > 0 { (current * 100) / total } else { 0 };
        let bar_width: usize = 18;
        let filled_blocks = if total > 0 { (current * bar_width) / total } else { 0 };
        
        let filled = "█".repeat(filled_blocks);
        let empty = "░".repeat(bar_width - filled_blocks);

        print!(
            "\r   ⤷  {:<14} [{}{}] {:3}% ({}/{})",
            prefix, filled, empty, percentage, current, total
        );
        let _ = io::stdout().flush();
    }

    pub fn prompt_confirmation(prompt: &str) -> bool {
        print!("  ? {} [y/N] ❯ ", prompt);
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return false;
        }

        let trimmed = input.trim().to_lowercase();
        trimmed == "y" || trimmed == "yes"
    }

    pub fn render_list(title: &str, items: &[String]) {
        let width: usize = 50;
        let fill_len = width.saturating_sub(title.len() + 5);
        println!("\n  ┌── {} {}", title, "─".repeat(fill_len));
        
        if items.is_empty() {
            println!("  └─ (none)");
            return;
        }
        
        for (index, item) in items.iter().enumerate() {
            if index == items.len() - 1 {
                println!("  └─ {}", item);
            } else {
                println!("  ├─ {}", item);
            }
        }
    }

    pub fn table(title: &str, headers: &[&str], rows: &[Vec<String>]) {
        if headers.is_empty() { return; }
        
        let mut widths = vec![0; headers.len()];
        for (i, h) in headers.iter().enumerate() {
            widths[i] = h.len();
        }
        for row in rows {
            for (i, val) in row.iter().enumerate() {
                if i < widths.len() && val.len() > widths[i] {
                    widths[i] = val.len();
                }
            }
        }

        let total_width: usize = widths.iter().map(|w| w + 3).sum::<usize>() + 1;
        let fill_len = total_width.saturating_sub(title.len() + 5);
        println!("\n  ┌── {} {}", title, "──".repeat(fill_len));

        print!("  │ ");
        for (i, h) in headers.iter().enumerate() {
            print!("{::<width$} │ ", h, width = widths[i]);
        }
        println!();

        print!("  ├──");
        for (i, w) in widths.iter().enumerate() {
            print!("{}", "─".repeat(*w));
            if i == widths.len() - 1 {
                print!("─┤");
            } else {
                print!("─┼─");
            }
        }
        println!();

        for row in rows {
            print!("  │ ");
            for (i, val) in row.iter().enumerate() {
                if i < widths.len() {
                    print!("{::<width$} │ ", val, width = widths[i]);
                }
            }
            println!();
        }

        print!("  └──");
        for (i, w) in widths.iter().enumerate() {
            print!("{}", "─".repeat(*w));
            if i == widths.len() - 1 {
                print!("──┘");
            } else {
                print!("─┴─");
            }
        }
        println!();
    }

    pub fn render_key_values(title: &str, pairs: &[(&str, &str)]) {
        if pairs.is_empty() { return; }
        
        let mut max_key_len = 0;
        for (k, _) in pairs {
            if k.len() > max_key_len {
                max_key_len = k.len();
            }
        }

        let width: usize = 50;
        let fill_len = width.saturating_sub(title.len() + 5);
        println!("\n  ┌── {} {}", title, "─".repeat(fill_len));

        for (index, (k, v)) in pairs.iter().enumerate() {
            if index == pairs.len() - 1 {
                println!("  └── {:<width$} : {}", k, v, width = max_key_len);
            } else {
                println!("  ├── {:<width$} : {}", k, v, width = max_key_len);
            }
        }
    }

    pub fn block(title: &str, lines: &[&str]) {
        let width: usize = 60;
        let fill_len = width.saturating_sub(title.len() + 5);
        println!("\n  ┌── {} {}", title, "─".repeat(fill_len));
        for line in lines {
            println!("  │ {}", line);
        }
        println!("  └──{}", "─".repeat(width - 3));
    }

    pub fn separator() {
        println!("\n  ─{}", "─".repeat(70));
    }
}
