use anyhow::Result;
use crate::core::db::Database;

/// Canonical mcx subcommands (mirrors the Cli enum in main.rs).
const SUBCOMMANDS: &[&str] = &[
    "install", "add", "remove", "purge", "search", "update", "upgrade",
    "query", "clean", "verify", "fix", "config", "history", "build",
    "repo-add", "repo-remove", "repo-list", "repo-sync", "repo-enable",
    "repo-disable", "repo-info", "self-update", "vendor", "completion",
    "cgroup", "mode",
];

pub struct CompletionEngine {
    db: std::sync::Arc<Database>,
}

impl CompletionEngine {
    pub fn new(db: std::sync::Arc<Database>) -> Self {
        Self { db }
    }

    pub fn complete_subcommand(&self, current_token: &str) -> Vec<String> {
        SUBCOMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(current_token))
            .map(|cmd| cmd.to_string())
            .collect()
    }

    pub fn complete_installed_package(&self, current_token: &str) -> Result<Vec<String>> {
        let installed = self.db.get_all_installed_packages()?;

        Ok(installed
            .into_iter()
            .filter(|pkg| pkg.pkg_name.starts_with(current_token))
            .map(|pkg| pkg.pkg_name)
            .collect())
    }

    pub fn complete_remote_package(&self, current_token: &str) -> Result<Vec<String>> {
        let available = self.db.get_all_available_packages()?;

        Ok(available
            .into_iter()
            .filter(|pkg| pkg.pkg_name.starts_with(current_token))
            .map(|pkg| pkg.pkg_name)
            .collect())
    }

    pub fn generate_shell_blueprint(&self, shell_type: &str) -> Result<String> {
        match shell_type.to_lowercase().as_str() {
            "bash" => Ok(self.bash_template()),
            "zsh" => Ok(self.zsh_template()),
            "fish" => Ok(self.fish_template()),
            _ => Err(anyhow::anyhow!("Unsupported shell type '{}' (expected bash, zsh, or fish)", shell_type)),
        }
    }

    fn bash_template(&self) -> String {
        let opts = SUBCOMMANDS.join(" ");
        format!(
            r#"_mcx_completions() {{
    local cur
    COMPREPLY=()
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    if [[ ${{COMP_CWORD}} -eq 1 ]] ; then
        COMPREPLY=( $(compgen -W "{opts}" -- "${{cur}}") )
        return 0
    fi
}}
complete -F _mcx_completions mcx"#
        )
    }

    fn zsh_template(&self) -> String {
        let mut out = String::from("#compdef mcx\n_mcx_commands() {\n    local -a commands\n    commands=(\n");
        for cmd in SUBCOMMANDS {
            out.push_str(&format!("        '{cmd}'\n"));
        }
        out.push_str(
            r#"    )
    _describe "mcx commands" commands
}
_mcx"#,
        );
        out
    }

    fn fish_template(&self) -> String {
        let mut out = String::from("complete -c mcx -f\n");
        for cmd in SUBCOMMANDS {
            out.push_str(&format!("complete -c mcx -n \"__fish_use_subcommand\" -a {cmd}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_db(root: &std::path::Path) -> std::sync::Arc<Database> {
        let _ = fs::remove_dir_all(root);
        std::fs::create_dir_all(root).unwrap();
        std::sync::Arc::new(Database::open(root).expect("open test database"))
    }

    #[test]
    fn test_completions_list_real_subcommands() {
        let root = std::env::temp_dir().join(format!("mcx_test_completion_{}", std::process::id()));
        let engine = CompletionEngine::new(test_db(&root));
        let all = engine.complete_subcommand("");
        for expected in ["install", "remove", "repo-add", "self-update", "cgroup"] {
            assert!(all.iter().any(|c| c == expected), "missing subcommand {}", expected);
        }
        // Phantom commands from the old templates must be gone.
        for phantom in ["generate", "rebuild", "audit", "fix-deps"] {
            assert!(!all.iter().any(|c| c == phantom), "phantom subcommand {}", phantom);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_shell_templates_contain_real_subcommands() {
        let root = std::env::temp_dir().join(format!("mcx_test_completion_tpl_{}", std::process::id()));
        let engine = CompletionEngine::new(test_db(&root));
        for shell in ["bash", "zsh", "fish"] {
            let script = engine.generate_shell_blueprint(shell).expect(shell);
            assert!(script.contains("repo-sync"), "{} template missing repo-sync", shell);
            assert!(script.contains("self-update"), "{} template missing self-update", shell);
            assert!(!script.contains("fix-deps"), "{} template contains phantom fix-deps", shell);
        }
        assert!(engine.generate_shell_blueprint("tcsh").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
