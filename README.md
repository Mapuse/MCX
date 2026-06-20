#

`▐▀` `-` `▀▀▀▀▀▀▀▀▌`

```shell
███╗   ███╗  ██████╗██╗    ██╗    ██████╗  █████╗  ██████╗██╗  ██╗ █████╗  ██████╗ ███████╗
████╗ ████║██╔════╝ ╚██╗  ██╔╝     ██╔══██╗██╔══██╗██╔════╝██║ ██╔╝██╔══██╗██╔════╝ ██╔════╝
██╔████╔██║██║        ╚███╔╝      ██████╔╝███████║██║     █████╔╝ ███████║██║  ███╗█████╗  
██║╚██╔╝██║██║      ██╔   ██╗     ██╔═══╝ ██╔══██║██║     ██╔═██╗ ██╔══██║██║   ██║██╔══╝  
██║ ╚═╝ ██║╚██████╗██╔╝    ██╗    ██║     ██║  ██║╚██████╗██║  ██╗██║  ██║╚██████╔╝███████╗
╚═╝     ╚═╝ ╚═════╝╚═╝     ╚═╝    ╚═╝     ╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝
                                                                                        
███╗   ███╗ █████╗ ███╗   ██╗ █████╗  ██████╗ ███████╗██████╗                           
████╗ ████║██╔══██╗████╗  ██║██╔══██╗██╔════╝ ██╔════╝██╔══██╗                          
██╔████╔██║███████║██╔██╗ ██║███████║██║  ███╗█████╗  ██████╔╝                          
██║╚██╔╝██║██╔══██║██║╚██╗██║██╔══██║██║   ██║██╔══╝  ██╔══██╗                          
██║ ╚═╝ ██║██║  ██║██║ ╚████║██║  ██║╚██████╔╝███████╗██║  ██║                          
╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝                          
```

`▀` `-` `▀▀▀▀`

<details><summary id="contents">Contents</summary>
  
- [[Architecture]](#arch)
- [[Commands]](#commands)
  - [[Package Management]](#package-management)
  - [[Repository Management]](#repository-management)
- [[Integration]](#integration)
- [[Development]](#development)
- [[Building]](#building)
- [[Credits]](#credits)
- [[License]](#license)

</details>

<details><summary id="commands">Commands</summary>

## Package Management

| Command | Aliases | Description |
| --------- | --------- | ------------- |
| `install` | `i`, `in`, `add` | Deploy packages into target system |
| `add-local` | `a`, `local`, `package`, `xcs` | Install local .xcs package file |
| `remove` | `r`, `rm`, `uninstall`, `delete` | Remove installed packages |
| `search` | `s`, `find`, `look` | Search available packages |
| `update` | `u`, `refresh`, `sync` | Sync repository indexes |
| `upgrade` | `U`, `up`, `dist-upgrade` | Upgrade all installed packages |
| `query` | `q`, `info`, `show` | Show package information |
| `clean` | `c`, `wipe`, `clear` | Clear cache and temporary files |
| `verify` | `v`, `check`, `certify` | Verify package integrity |
| `fix-deps` | `f`, `fix-deps`, `repair` | Fix dependency issues |
| `config` | `C`, `cfg`, `settings` | Manage MCX configuration |
| `history` | `h`, `log`, `record` | Show installation history |
| `build` | `b`, `make`, `create` | Build system from blueprint |

## Repository Management

| Command | Aliases | Description |
| --------- | --------- | ------------- |
| `repo-add` | `ra` | Add a repository |
| `repo-remove` | `rr` | Remove a repository |
| `repo-list` | `rl` | List configured repositories |

</details>

<details><summary id="integration">Integration</summary>

## Development

### Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Check without building
cargo check
```

</details>

<details><summary id="credits">Credits</summary>

**`MCX`** is part of the **`Cudane` Linux** ecosystem, designed to work seamlessly with **`rLine`** build engine.

- **`Cudane`** - The Linux Distribution.
- **`rLine`** - Source-To-Archive Build Engine.
- **`MCX`** - Runtime Package Manager.

</details>

<details><summary id="license">License</summary>

The Unlicebse - see [[**`LICENSE`**](github.com/Cudane/MCX/LICENSE)] file for details.

</details>

`-` `▄▄▄▄▄▄▄▄▌`

##

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`MCX`** is the official package manager for **`Cudane`**, completely written as a **`Runtime Package Manager`** to achieve 100% compatibility with the **`rLine`** build engine.
  
`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`Version`:** **`2.7.5`**.
- **`Engine`:** **`rLine 0.2.0`**.
- **`Architecture`:** **`x86_64-unknown-linux-musl`** (**`x86_64-pc-linux-musl`**).
- **`Compression`:** **`Zstd`**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`
