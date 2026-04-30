## Overview

`cim merge` combines two or more SDK targets into a single `sdk.yml`,
along with merged `os-dependencies.yml` and `python-dependencies.yml`
when present. The command is useful when smaller, focused targets need
to be composed into a larger workspace without manual copy-paste.

## Usage

```bash
cim merge --targets TARGET1,TARGET2 --output DIR \
          [--source URL|PATH] [--mirror PATH] \
          [--fragment PATH] [--dry-run]
```

## Options

| Flag | Short | Description |
|------|-------|-------------|
| `--targets` | `-t` | Comma/space-separated target names (minimum 2) |
| `--output` | `-o` | Directory where merged files are written |
| `--source` | `-s` | Manifest repo URL or local path (default from config) |
| `--mirror` | | Override the mirror path in the merged config |
| `--fragment` | | Apply a fragment file to the merged result (repeatable) |
| `--dry-run` | | Print merged YAML to stdout without writing files |

## Merge Semantics

### List sections (gits, toolchains, copy_files, install)

Items are matched by their unique key:

- **gits**: matched by `name`
- **toolchains**: matched by `get_name()` (name or URL-derived)
- **copy_files**: matched by `dest`
- **install**: matched by `name`

When the same key appears in multiple targets:

- **Identical entries** (PartialEq): silently deduplicated, kept once.
- **Different entries**: reported as a merge conflict. The merge aborts
  with an error listing all conflicts. The user must resolve them by
  renaming items in the source targets or applying a fragment that
  overrides the conflicting entry.

### Scalar fields

- **mirror**: taken from the first target. A note is printed if later
  targets differ. Override with `--mirror`.
- **build_folder**: first non-None value wins. A note is printed if
  later targets differ.

### Variables

Combined from all targets. Same key with different values is a
conflict (same key with identical value is silently merged).

### Makefile include

`files` and `exclude` lists from all targets are concatenated and
deduplicated.

### Global targets (build, test, clean, flash, envsetup)

These are **omitted** from the merged config. Since each source target
may define conflicting build recipes, there is no safe automatic merge.
Instead:

- The merged YAML contains **commented-out suggestions** showing each
  source target's commands (e.g. `# From target1: ...`).
- Users provide the actual global targets via a `--fragment` flag or
  by adding a fragment to the workspace after init.
- If a fragment supplies a global target, it appears as active YAML in
  the output; the remaining undefined targets still appear as
  commented suggestions.

### os-dependencies.yml

OS dependency files from each target are merged. For each
platform/distro combination, package lists are combined and
deduplicated while preserving order. The merged file is written to the
output directory only if at least one source target has it.

### python-dependencies.yml

Profile packages are merged across targets: same profile names have
their package lists combined and deduplicated. The `default` profile
is taken from the first target that defines one. Written only if at
least one source target has it.

## YAML Output Format

The merged `sdk.yml` is produced by a custom YAML emitter
(`format_merged_yaml()` in [dsdk-cli/src/merge.rs](dsdk-cli/src/merge.rs))
rather than serde_yaml, to match the hand-written style of existing
manifests:

- Section dividers (`###...`) with comment headers
- Header comment listing merged targets and the command used
- No `null` values; empty optional sections are omitted entirely
- Values starting with YAML-reserved characters (`@`, `` ` ``, `{`,
  `}`, `*`, `&`, `!`, `%`, `#`, `|`, `>`, `'`, `"`, `[`, `]`, `,`,
  `?`) are automatically double-quoted via `yaml_quote()`
- Block scalars (e.g. multi-line `commands: |`) are preserved
- Variables sorted alphabetically for deterministic output

## Conflict Resolution

When `cim merge` reports conflicts:

1. **Rename** one of the conflicting items in the source target so
   they no longer collide.
2. **Use a fragment** (`--fragment override.yml`) that defines the
   correct version of the conflicting item. The fragment is applied
   after the merge, so it replaces the first-wins entry.

## Fragment Integration

Fragments passed via `--fragment` are applied to the merged
`SdkConfig` in order, after the merge but before validation and YAML
emission. This means a fragment can:

- Override a conflicting git/toolchain entry
- Supply global targets (build, test, clean, flash, envsetup)
- Add extra gits, toolchains, variables, or copy_files
- Remove items via `remove_gits`, `remove_toolchains`, etc.

## Validation

After merging (and applying fragments), the config is validated for:

- Dangling `build_depends_on` references (git names that don't exist)
- Dangling `git_depends_on` references
- Empty gits list

Warnings are printed but do not abort the merge.

## Example

```bash
# Merge two targets, override mirror, supply global targets via fragment
cim merge \
  --targets base-sdk,extra-libs \
  --output ~/devel/cim-manifests/targets/combined \
  --mirror $HOME/tmp/mirror \
  --fragment build-targets.yml

# Preview without writing files
cim merge --targets a,b --output /tmp/test --dry-run
```

## Key Source Files

| File | Purpose |
|------|---------|
| [dsdk-cli/src/merge.rs](dsdk-cli/src/merge.rs) | Core merge logic, YAML emitter, conflict detection |
| [dsdk-cli/src/fragment.rs](dsdk-cli/src/fragment.rs) | Fragment application (used by `--fragment`) |
| [dsdk-cli/src/cli.rs](dsdk-cli/src/cli.rs) | CLI definition (`Merge` variant in `Commands` enum) |
| [dsdk-cli/src/main.rs](dsdk-cli/src/main.rs) | `handle_merge_command()` orchestration |
| [dsdk-cli/src/config.rs](dsdk-cli/src/config.rs) | `SdkConfig` and related structs (with PartialEq) |
| [dsdk-cli/tests/merge_tests.rs](dsdk-cli/tests/merge_tests.rs) | Unit tests for merge logic |
