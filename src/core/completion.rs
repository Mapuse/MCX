use anyhow::Result;
use crate::core::db::Database;

pub struct CompletionEngine {
    db: std::sync::Arc<Database>,
}

impl CompletionEngine {
    pub fn new(db: std::sync::Arc<Database>) -> Self {
        Self { db }
    }

    pub fn complete_subcommand(&self, current_token: &str) -> Vec<String> {
        let subcommands = vec![
            "install", "add-local", "remove", "search", "update",
            "upgrade", "query", "clean", "verify", "fix-deps",
            "config", "generate", "history", "rebuild", "audit"
        ];

        subcommands
            .into_iter()
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
            _ => Err(anyhow::anyhow!("Unsupported target infrastructure shell type")),
        }
    }

    fn bash_template(&self) -> String {
        r#"_mcx_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="install add-local remove search update upgrade query clean verify fix-deps config generate history rebuild audit"

    if [[ ${COMP_CWORD} -eq 1 ]] ; then
        COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
        return 0
    fi
}
complete -F _mcx_completions mcx"# .to_string()
    }

    fn zsh_template(&self) -> String {
        r#"#compdef mcx
_mcx() {
    local line
    _arguments -C \
        "1: :_mcx_commands" \
        "*::arg:->args"

    case $line[1] in
        *)
            _message "compiler completion dynamic engine active"
            ;;
    esac
}
_mcx_commands() {
    local -a commands
    commands=(
        'install:Deploy assets into target node'
        'add-local:Inject immediate structural block file'
        'remove:Purge entity branch and link dependencies'
        'search:Query index registry maps'
        'update:Pull remote manifest mutations'
        'upgrade:Execute global system alignment pipeline'
        'query:Inspect specific ledger status node'
        'clean:Evict transient caching files'
        'verify:Audit file matrix allocations'
        'fix-deps:Resolve dead link structures'
        'config:Mutate baseline engine preferences'
        'generate:Build structural template profiles'
        'history:Roll back tracking timeline chains'
        'rebuild:Synchronize system node to schema profile'
        'audit:Evaluate neutrality metric baselines'
    )
    _describe "mcx commands" commands
}
_mcx"# .to_string()
    }

    fn fish_template(&self) -> String {
        r#"complete -c mcx -f
complete -c mcx -n "__fish_use_subcommand" -a install -d 'Deploy assets into target node'
complete -c mcx -n "__fish_use_subcommand" -a add-local -d 'Inject immediate structural block file'
complete -c mcx -n "__fish_use_subcommand" -a remove -d 'Purge entity branch and link dependencies'
complete -c mcx -n "__fish_use_subcommand" -a search -d 'Query index registry maps'
complete -c mcx -n "__fish_use_subcommand" -a update -d 'Pull remote manifest mutations'
complete -c mcx -n "__fish_use_subcommand" -a upgrade -d 'Execute global system alignment pipeline'
complete -c mcx -n "__fish_use_subcommand" -a query -d 'Inspect specific ledger status node'
complete -c mcx -n "__fish_use_subcommand" -a clean -d 'Evict transient caching files'
complete -c mcx -n "__fish_use_subcommand" -a verify -d 'Audit file matrix allocations'
complete -c mcx -n "__fish_use_subcommand" -a fix-deps -d 'Resolve dead link structures'
complete -c mcx -n "__fish_use_subcommand" -a repo-add -d 'Add a repository'
complete -c mcx -n "__fish_use_subcommand" -a repo-remove -d 'Remove a repository'
complete -c mcx -n "__fish_use_subcommand" -a repo-list -d 'List configured repositories'
complete -c mcx -n "__fish_use_subcommand" -a config -d 'Mutate baseline engine preferences'
complete -c mcx -n "__fish_use_subcommand" -a generate -d 'Build structural template profiles'
complete -c mcx -n "__fish_use_subcommand" -a history -d 'Roll back tracking timeline chains'
complete -c mcx -n "__fish_use_subcommand" -a rebuild -d 'Synchronize system node to schema profile'
complete -c mcx -n "__fish_use_subcommand" -a audit -d 'Evaluate neutrality metric baselines'"# .to_string()
    }
}