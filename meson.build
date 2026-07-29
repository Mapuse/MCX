project('mcx', 'rust',
  version : '7.0.0',
  license : 'unlicense',
  default_options : [
    'optimization=3',
    'strip=true',
  ],
)

# ── Auto-detect architecture via gen-cross.sh ──────────────────────────
gen_cross = find_program('gen-cross.sh')
run_command(gen_cross, check : true)

build_dir = meson.project_build_root() / 'cargo'
src_dir = meson.project_source_root()

custom_target('mcx',
  build_by_default : true,
  build_always_stale : true,
  install : true,
  install_dir : get_option('prefix') / 'bin',
  command : ['sh', '-c',
    'CARGO_TARGET_DIR={0} cargo build --release --locked --manifest-path {1}/Cargo.toml && cp {0}/release/mcx @OUTPUT@'
    .format(build_dir, src_dir),
  ],
  output : 'mcx',
)
