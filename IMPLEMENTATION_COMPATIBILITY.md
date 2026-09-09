# Implementation Compatibility Audit — Outsider (ous 0.7.0) ⇄ MCX (mcx 7.0.0)

Static source audit of the from-bit pipeline: `/home/m/Outsider` (producer) → `.xcs` package + repository index → `/home/m/MCX` (consumer/installer). Both git trees clean at audit time. All rows re-assessed against current code.

## Established contract

| Term | Meaning |
|------|---------|
| **Index** | The packages registry/database: Outsider writes `index.{arch}.json` (arch = `x86_64`/`aarch64`/`native`), mcx syncs it into its local DB. Meta for installs comes from the solver/index. |
| **metadata.json** | Lives **inside** the `.xcs` archive (tar root `"./"`). Per-package metadata. Its `checksum` field is the object `{"kind","value"}` = hash of the **UNCOMPRESSED** tar stream (informational on the consumer side). |
| **Transport integrity** | Sidecar `<name>-<ver>.xcs.sha256` (single-line hex sha256 of **COMPRESSED** `.xcs` bytes). mcx verifies against the sidecar; also downloads it as sibling URL `source + ".sha256"`. Sidecar missing → tolerated with warning. |

## Audit table

| # | Contract | Status | Evidence |
|---|----------|--------|----------|
| 1 | **Layout & install surface** (`/system` prefix, sidecars, no `fus`) | ✔ AGREE | Outsider packages `$(DESTDIR)$(PREFIX)` with `PREFIX ?= /system`. MCX places each archive entry at `root.join(relpath)` via `tar::Archive`/`zstd` (`archive/extract.rs:12-16`). Sidecar format `{hash}\n` matches `HashVerifier` (`archive/hash.rs:14-48`). No `fus`/`full-install` artifact in MCX source; state lives under `var/lib/mcx/*` (`core/constants.rs`). |
| 2 | **Host architecture & index/pool file naming** | ✔ FIXED | Outsider `canonical_arch()` (`lib.rs:91-105`) maps `amd64→x86_64`, `arm64→aarch64`, passes through `x86_64`/`aarch64`/`native`, rejects all else. Index filenames use canonical set: `index.{arch}.json`. MCX `Architecture::short_name()` (`arch.rs:41-47`) returns the same set. MCX `PackageMetadata` (`database.rs:43-44`) has `#[serde(default, alias = "arch")]` on `architecture`. MCX `PackageEntity` (`package.rs:24`) has `#[serde(default, alias = "arch")]` on `architecture`. Both sides agree on `{x86_64, aarch64, native}`. |
| 3 | **Checksum semantics: embedded vs sidecar vs index** | ✔ FIXED | **Embedded** (metadata.json inside archive): `checksum.{kind,value}` = hash of the UNCOMPRESSED tar stream — **informational** per contract; mcx does NOT verify against this at install time. **Sidecar** (`*.xcs.sha256`): single-line hex sha256 of COMPRESSED `.xcs` bytes — **verified** by both `install.rs:334-348` and `add.rs:58-81`. **Index** `checksum.{kind,value}`: compressed-bytes hash (informational, not verified by mcx). Contract is respected: embedded is informational, sidecar provides transport integrity. |
| 4 | **Embedded metadata.json schema** | ✔ FIXED | Outsider embeds `checksum` as `{"kind","value"}` object (`lib.rs:306-310`). MCX `add.rs:33-49` uses `serde_json::Value` to parse both flat-string and `{kind,value}` object forms. MCX `package.rs:flexible_checksum` module (line 12-55) provides `#[serde(with)]` for `PackageEntity.checksum` — deserializes from either string or `{kind,value}` object, serializes to object form. `ManifestParser::parse_embedded_manifest` (`manifest.rs:9-23`) now accepts Outsider-shaped metadata.json. Test `test_manifest_parser_accepts_outsider_checksum_object` confirms both shapes. |
| 5 | **metadata.json never lands on the live system** | ✔ FIXED | `Extractor::extract_zstd_archive` (`archive/extract.rs:45-47`) skips `metadata.json` at tar root `"./"`. `AddLocalCommand::execute` (`commands/add.rs:79-81`) skips `metadata.json` during extraction. Both install paths (repo via `install.rs:357` → `Extractor`, and local via `add.rs:73-87`) filter it. Test `test_outsider_round_trip_local_install` asserts `!root.join("metadata.json").exists()`. |
| 6 | **cps engine, python/plugins, tooling parity** | ✔ AGREE | Both crates pin identical cps rev `c4ba21e185398558052acec3f0b4619b4e8c0678` with `default-features = false` (Outsider `Cargo.toml:29`, MCX `Cargo.toml:51`). Both `Cargo.lock` resolve cps `0.0.70`. `python = ["cps/python"]` is opt-in on both sides. External tool requirements match. No shared numeric constants exist. |
| 7 | **Round-trip test coverage** | ✔ FIXED | `tests/integration.rs:test_outsider_round_trip_local_install` constructs an Outsider-shaped `.xcs` archive (zstd-compressed tar with `{kind,value}` checksum, `architecture` field, `metadata.json`), writes the `.sha256` sidecar, installs via `AddLocalCommand`, and asserts: (a) metadata.json deserializes, (b) install succeeds, (c) sidecar hash matches, (d) no `metadata.json` on live root, (e) `architecture` consumed. `test_manifest_parser_accepts_outsider_checksum_object` confirms `ManifestParser` accepts both Outsider object and legacy flat-string checksums. |

## Changes made (file:line)

| File | Line(s) | Change |
|------|---------|--------|
| `src/commands/add.rs:33-49` | `EmbeddedMeta` deserialization rewritten: uses `serde_json::Value` to parse both flat-string and `{kind,value}` object checksum; extracts `architecture`/`arch` field |
| `src/commands/add.rs:57-81` | Sidecar verification replaces embedded-checksum verification: reads `.xcs.sha256` sidecar, warns if missing |
| `src/commands/add.rs:79-81` | `metadata.json` filtered from extraction loop in `AddLocalCommand` |
| `src/commands/add.rs:139` | `architecture` from embedded metadata used instead of hardcoded `"native"` |
| `src/commands/install.rs:334-348` | Sidecar verification emits `UserInterface::warning` when `.xcs.sha256` is missing/unreadable |
| `src/core/package.rs:12-55` | New `flexible_checksum` module: `#[serde(with)]` handler that deserializes checksum from string or `{kind,value}` object, serializes to object form |
| `src/core/package.rs:64` | `PackageEntity.checksum` uses `#[serde(with = "flexible_checksum")]` |
| `src/core/package.rs:199-216` | Updated test to verify both Outsider object and legacy flat-string checksum deserialization |
| `src/archive/extract.rs:45-47` | Already present: `metadata.json` skipped during extraction (pre-existing fix) |
| `src/core/database.rs:43-44` | Already present: `#[serde(alias = "arch")]` on `architecture` field (pre-existing fix) |
| `tests/integration.rs` | Two new tests: `test_outsider_round_trip_local_install`, `test_manifest_parser_accepts_outsider_checksum_object` |

## Verdict

**All interlock points are compatible.** An Outsider-produced `.xcs` package installs cleanly through both `mcx add` (local) and `mcx install` (repo) paths. The embedded `metadata.json` deserializes correctly (both `{kind,value}` object and legacy flat-string forms), `architecture` is consumed, `metadata.json` never lands on the live system, and transport integrity is verified against the sidecar. No outstanding mismatches remain.
