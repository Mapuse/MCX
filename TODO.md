# TODO - MCX TUI + wallust integration

- [ ] Step 1: Rewrite `src/utils/ui.rs` into a complete ratatui + crossterm + wallust themed UI/dialog+toasts system.
- [ ] Step 2: Expose an easy-to-use `UiSession` / API from `utils::ui`.
- [ ] Step 3: Integrate UI into `src/main.rs` (replace direct println/eprintln + route all output through UI).
- [ ] Step 4: Integrate UI into src/commands and src/core and src/archive and src/network and src/utils.
- [ ] Step 5: Add progress/dialog reporting hooks for `install.rs`, `remove.rs`, and `sync.rs`.
- [ ] Step 6: Ensure non-interactive fallback works (CI/no TTY).
- [ ] Step 7: Update `README.md` with complete UX + wallust theming + keybindings.
- [ ] Step 8: don't build, just test.
