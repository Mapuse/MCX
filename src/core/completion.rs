use anyhow::Result;
use crate::core::database::Database;

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
            "config", "history", "build", "repo-add", "repo-remove",
            "repo-list", "lazy-mount", "lazy-umount", "dedup", "rollback",
            "generations", "delta", "checkpoint", "snapshots", "stream-mount",
            "stream-umount", "overlay-create", "overlay-remove", "swarm-hash",
            "swarm-get", "swarm-peers", "swarm-peer-add", "throttle-set", "throttle-remove"
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
    opts="install add-local remove search update upgrade query clean verify fix-deps config history build repo-add repo-remove repo-list lazy-mount lazy-umount dedup rollback generations delta checkpoint snapshots stream-mount stream-umount overlay-create overlay-remove swarm-hash swarm-get swarm-peers swarm-peer-add throttle-set throttle-remove"

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
        'install:Deploy packages into target system'
        'add-local:Install local .xcs package file'
        'remove:Remove installed packages'
        'search:Search available packages'
        'update:Sync repository indexes'
        'upgrade:Upgrade all installed packages'
        'query:Show package information'
        'clean:Clear cache and temporary files'
        'verify:Verify package integrity'
        'fix-deps:Fix dependency issues'
        'config:Manage MCX configuration'
        'history:Show installation history'
        'build:Build system from blueprint'
        'repo-add:Add a repository'
        'repo-remove:Remove a repository'
        'repo-list:List configured repositories'
        'lazy-mount:Enable lazy mounting for package'
        'lazy-umount:Disable lazy mounting'
        'dedup:Run CAS deduplication'
        'rollback:Rollback to previous generation'
        'generations:List package generations'
        'delta:Reconstruct package from delta'
        'checkpoint:Create process snapshot'
        'snapshots:List process snapshots'
        'stream-mount:Mount remote package stream'
        'stream-umount:Unmount stream'
        'overlay-create:Create isolated overlay'
        'overlay-remove:Remove isolated overlay'
        'swarm-hash:Register swarm hash'
        'swarm-get:Get swarm hash'
        'swarm-peers:List swarm peers'
        'swarm-peer-add:Add swarm peer'
        'throttle-set:Set resource limits'
        'throttle-remove:Remove resource limits'
    )
    _describe "mcx commands" commands
}
_mcx"# .to_string()
    }

    fn fish_template(&self) -> String {
        r#"complete -c mcx -f
complete -c mcx -n "__fish_use_subcommand" -a install -d 'Deploy packages into target system'
complete -c mcx -n "__fish_use_subcommand" -a add-local -d 'Install local .xcs package file'
complete -c mcx -n "__fish_use_subcommand" -a remove -d 'Remove installed packages'
complete -c mcx -n "__fish_use_subcommand" -a search -d 'Search available packages'
complete -c mcx -n "__fish_use_subcommand" -a update -d 'Sync repository indexes'
complete -c mcx -n "__fish_use_subcommand" -a upgrade -d 'Upgrade all installed packages'
complete -c mcx -n "__fish_use_subcommand" -a query -d 'Show package information'
complete -c mcx -n "__fish_use_subcommand" -a clean -d 'Clear cache and temporary files'
complete -c mcx -n "__fish_use_subcommand" -a verify -d 'Verify package integrity'
complete -c mcx -n "__fish_use_subcommand" -a fix-deps -d 'Fix dependency issues'
complete -c mcx -n "__fish_use_subcommand" -a config -d 'Manage MCX configuration'
complete -c mcx -n "__fish_use_subcommand" -a history -d 'Show installation history'
complete -c mcx -n "__fish_use_subcommand" -a build -d 'Build system from blueprint'
complete -c mcx -n "__fish_use_subcommand" -a repo-add -d 'Add a repository'
complete -c mcx -n "__fish_use_subcommand" -a repo-remove -d 'Remove a repository'
complete -c mcx -n "__fish_use_subcommand" -a repo-list -d 'List configured repositories'
complete -c mcx -n "__fish_use_subcommand" -a lazy-mount -d 'Enable lazy mounting for package'
complete -c mcx -n "__fish_use_subcommand" -a lazy-umount -d 'Disable lazy mounting'
complete -c mcx -n "__fish_use_subcommand" -a dedup -d 'Run CAS deduplication'
complete -c mcx -n "__fish_use_subcommand" -a rollback -d 'Rollback to previous generation'
complete -c mcx -n "__fish_use_subcommand" -a generations -d 'List package generations'
complete -c mcx -n "__fish_use_subcommand" -a delta -d 'Reconstruct package from delta'
complete -c mcx -n "__fish_use_subcommand" -a checkpoint -d 'Create process snapshot'
complete -c mcx -n "__fish_use_subcommand" -a snapshots -d 'List process snapshots'
complete -c mcx -n "__fish_use_subcommand" -a stream-mount -d 'Mount remote package stream'
complete -c mcx -n "__fish_use_subcommand" -a stream-umount -d 'Unmount stream'
complete -c mcx -n "__fish_use_subcommand" -a overlay-create -d 'Create isolated overlay'
complete -c mcx -n "__fish_use_subcommand" -a overlay-remove -d 'Remove isolated overlay'
complete -c mcx -n "__fish_use_subcommand" -a swarm-hash -d 'Register swarm hash'
complete -c mcx -n "__fish_use_subcommand" -a swarm-get -d 'Get swarm hash'
complete -c mcx -n "__fish_use_subcommand" -a swarm-peers -d 'List swarm peers'
complete -c mcx -n "__fish_use_subcommand" -a swarm-peer-add -d 'Add swarm peer'
complete -c mcx -n "__fish_use_subcommand" -a throttle-set -d 'Set resource limits'
complete -c mcx -n "__fish_use_subcommand" -a throttle-remove -d 'Remove resource limits'"# .to_string()
    }
}