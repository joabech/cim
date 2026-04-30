## Overview

Fragments are `.yml` files that overlay a base `sdk.yml` at runtime.
They let you customize a workspace without editing the original
manifest. Typical uses: adding extra gits, swapping a toolchain
version, supplying global build targets for a merged config, or
removing components you don't need.

Fragments are stored in `.cim/fragments/` inside the workspace and
are automatically discovered and applied by `cim update` and
`cim makefile`. They can also be passed explicitly via the
`--fragment` flag on `cim init`, `cim update`, `cim makefile`, and
`cim merge`.

## Fragment File Format

A fragment is a YAML file with the same field names as `sdk.yml`, but
every field is optional. Only the fields present in the fragment are
applied; everything else in the base config is left untouched.

```yaml
fragment:
  name: my-customization
  description: Add an extra git and override the build target

gits:
  - name: my-extra-repo
    url: https://github.com/org/repo.git
    commit: main

build:
  - make -C my-extra-repo all
```

### Metadata

The optional `fragment:` block provides human-readable metadata:

```yaml
fragment:
  name: descriptive-name
  description: What this fragment does
```

This metadata is shown by `cim fragment list`.

### Supported Fields

| Field | Merge behavior |
|-------|---------------|
| `gits` | Matched by `name`. Existing entries are deep-merged (only specified sub-fields override). New entries are appended. |
| `remove_gits` | List of git names to remove from base config. |
| `toolchains` | Matched by name/URL. Deep-merged or appended. |
| `remove_toolchains` | List of toolchain names to remove. |
| `variables` | Shallow merge; fragment values win on collision. |
| `copy_files` | Appended to the base list. |
| `remove_copy_files` | Matched by `dest` and removed. |
| `install` | Matched by `name`. Deep-merged or appended. |
| `remove_install` | List of install target names to remove. |
| `build`, `test`, `clean`, `flash`, `envsetup` | Replace the global target entirely. |
| `mirror` | Replace the mirror path. |
| `build_folder` | Replace the build folder path. |
| `makefile_include` | `files` and `exclude` lists are appended to base. |

### Application Order

1. **Remove** operations run first (`remove_gits`, `remove_toolchains`,
   `remove_copy_files`, `remove_install`).
2. **Add/Modify** operations run second (gits, toolchains, variables,
   copy_files, install).
3. **Scalar replacements** (mirror, build_folder).
4. **Global target replacements** (build, test, clean, flash, envsetup).
5. **Makefile include** (appended).

### Deep Merge for List Items

When a fragment references an existing item (matched by name), only
the fields present in the fragment override the base. For example:

```yaml
# Only change the commit of an existing git, leave url/build/deps alone
gits:
  - name: optee_os
    commit: 4.5.0
```

New items must provide all required fields (e.g. `url` and `commit`
for a new git).

## Discovery and Ordering

`discover_fragments_in_dir()` scans `.cim/fragments/` for any file
ending in `.yml` or `.yaml`. Files are sorted alphabetically, so
**later files override earlier ones**. Use numeric prefixes for
deterministic ordering:

```
.cim/fragments/
  00-base-overrides.yml
  10-extra-repos.yml
  20-build-targets.yml
```

Fragment `20-build-targets.yml` can override anything set by
`00-base-overrides.yml`.

## Integration Points

Fragments are applied automatically in these commands:

- **`cim init`**: after cloning repos, before first makefile generation
  (supports `--fragment PATH` and `--no-fragments`)
- **`cim update`**: after updating repos, before makefile regeneration
  (supports `--fragment PATH` and `--no-fragments`)
- **`cim makefile`**: discovers and applies workspace fragments before
  generating the Makefile
- **`cim merge`**: `--fragment PATH` applies fragments to the merged
  result before writing output

## CLI Commands

### fragment list

Show all fragments in the current workspace's `.cim/fragments/`
directory, with their metadata (name, description) if available.

```bash
cim fragment list
```

### fragment show

Display the raw YAML contents of a fragment. Accepts a filename, a
path, or a fragment name (tries `<name>.fragment.yml` then exact
match in the fragments directory).

```bash
cim fragment show my-overrides.yml
cim fragment show /path/to/fragment.yml
```

### fragment add

Copy one or more fragment files into `.cim/fragments/`. The file is
validated (parsed as a Fragment) before copying.

```bash
cim fragment add overrides.yml extras.yml
cim fragment add --force overrides.yml   # overwrite existing
```

| Flag | Short | Description |
|------|-------|-------------|
| `--force` | `-f` | Overwrite existing fragment files |

### fragment remove

Remove fragments from `.cim/fragments/`.

```bash
cim fragment remove my-overrides.yml
cim fragment remove --all
cim fragment remove --interactive
```

| Flag | Short | Description |
|------|-------|-------------|
| `--all` | | Remove all fragments |
| `--interactive` | `-i` | Show numbered list, select by comma-separated numbers |

Interactive mode displays a numbered menu:

```
Select fragment(s) to remove (comma-separated numbers, or 'q' to cancel):
  1) 00-base-overrides.yml
  2) 10-extra-repos.yml
> 1,2
```

## Example Fragment

A fragment that adds a git, removes a toolchain, and supplies the
global build target:

```yaml
fragment:
  name: custom-build
  description: Add custom repo and simplify toolchain setup

gits:
  - name: custom-scripts
    url: https://github.com/org/scripts.git
    commit: main

remove_toolchains:
  - arm-gnu-toolchain-old

build:
  commands:
    - make -C custom-scripts all
  depends_on:
    - sdk-envsetup
```

## Key Source Files

| File | Purpose |
|------|---------|
| [dsdk-cli/src/fragment.rs](dsdk-cli/src/fragment.rs) | `apply_fragment()`, `apply_fragments()`, `discover_fragments_in_dir()`, deep-merge helpers, validation |
| [dsdk-cli/src/config.rs](dsdk-cli/src/config.rs) | `Fragment`, `FragmentMetadata`, `FragmentGitConfig`, `FragmentToolchainConfig` structs |
| [dsdk-cli/src/cli.rs](dsdk-cli/src/cli.rs) | `FragmentCommand` enum (List, Show, Add, Remove) |
| [dsdk-cli/src/main.rs](dsdk-cli/src/main.rs) | `handle_fragment_command()` CLI dispatch |
| [dsdk-cli/src/makefile.rs](dsdk-cli/src/makefile.rs) | Fragment discovery + application before Makefile generation |
