// Copyright (c) 2026 Analog Devices, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::Path;

use crate::config::{
    Fragment, FragmentGitConfig, FragmentToolchainConfig, GitConfig, InstallConfig,
    MakefileInclude, MakefileIncludeConfig, SdkConfig, ToolchainConfig,
};
use crate::messages;

/// Apply a single fragment on top of an SdkConfig, mutating it in place.
///
/// The merge semantics are:
/// - List items (gits, toolchains, install): matched by name, deep-merged if
///   found, appended if new.
/// - remove_* fields: remove matched items from the base config.
/// - Scalar fields (mirror, build_folder): replaced entirely.
/// - Global targets (build, test, clean, flash, envsetup): replaced entirely.
/// - Variables: shallow merge, fragment values win on collision.
/// - copy_files: appended.
/// - makefile_include: files and exclude lists are appended.
pub fn apply_fragment(
    config: &mut SdkConfig,
    fragment: &Fragment,
) -> Result<(), Box<dyn std::error::Error>> {
    let fragment_name = fragment
        .fragment
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .unwrap_or("<unnamed>");

    // --- Remove operations (applied before adds/modifies) ---

    if let Some(ref remove_gits) = fragment.remove_gits {
        for name in remove_gits {
            let before_len = config.gits.len();
            config.gits.retain(|g| g.name != *name);
            if config.gits.len() == before_len {
                messages::info(&format!(
                    "Fragment '{}': remove_gits '{}' not found in base config",
                    fragment_name, name
                ));
            }
        }
    }

    if let Some(ref remove_toolchains) = fragment.remove_toolchains {
        if let Some(ref mut toolchains) = config.toolchains {
            for name in remove_toolchains {
                let before_len = toolchains.len();
                toolchains.retain(|t| t.get_name() != *name);
                if toolchains.len() == before_len {
                    messages::info(&format!(
                        "Fragment '{}': remove_toolchains '{}' not found in base config",
                        fragment_name, name
                    ));
                }
            }
        }
    }

    if let Some(ref remove_copy_files) = fragment.remove_copy_files {
        if let Some(ref mut copy_files) = config.copy_files {
            for remove_entry in remove_copy_files {
                let before_len = copy_files.len();
                copy_files.retain(|cf| cf.dest != remove_entry.dest);
                if copy_files.len() == before_len {
                    messages::info(&format!(
                        "Fragment '{}': remove_copy_files dest '{}' not found in base config",
                        fragment_name, remove_entry.dest
                    ));
                }
            }
        }
    }

    if let Some(ref remove_install) = fragment.remove_install {
        if let Some(ref mut installs) = config.install {
            for name in remove_install {
                let before_len = installs.len();
                installs.retain(|i| i.name != *name);
                if installs.len() == before_len {
                    messages::info(&format!(
                        "Fragment '{}': remove_install '{}' not found in base config",
                        fragment_name, name
                    ));
                }
            }
        }
    }

    // --- Add/Modify operations ---

    if let Some(ref fragment_gits) = fragment.gits {
        for fg in fragment_gits {
            if let Some(existing) = config.gits.iter_mut().find(|g| g.name == fg.name) {
                merge_git(existing, fg);
            } else {
                // New git — url and commit are required
                let url = fg.url.as_ref().ok_or_else(|| {
                    format!(
                        "Fragment '{}': new git '{}' requires 'url' field",
                        fragment_name, fg.name
                    )
                })?;
                let commit = fg.commit.as_ref().ok_or_else(|| {
                    format!(
                        "Fragment '{}': new git '{}' requires 'commit' field",
                        fragment_name, fg.name
                    )
                })?;
                config.gits.push(GitConfig {
                    name: fg.name.clone(),
                    url: url.clone(),
                    commit: commit.clone(),
                    build_depends_on: fg.build_depends_on.clone(),
                    git_depends_on: fg.git_depends_on.clone(),
                    build: fg.build.clone(),
                    documentation_dir: fg.documentation_dir.clone(),
                });
            }
        }
    }

    if let Some(ref fragment_toolchains) = fragment.toolchains {
        let toolchains = config.toolchains.get_or_insert_with(Vec::new);
        for ft in fragment_toolchains {
            let ft_name = ft.get_name();
            if let Some(ref name) = ft_name {
                if let Some(existing) = toolchains.iter_mut().find(|t| t.get_name() == *name) {
                    merge_toolchain(existing, ft);
                } else {
                    // New toolchain — url and destination are required
                    let url = ft.url.as_ref().ok_or_else(|| {
                        format!(
                            "Fragment '{}': new toolchain '{}' requires 'url' field",
                            fragment_name, name
                        )
                    })?;
                    let destination = ft.destination.as_ref().ok_or_else(|| {
                        format!(
                            "Fragment '{}': new toolchain '{}' requires 'destination' field",
                            fragment_name, name
                        )
                    })?;
                    toolchains.push(ToolchainConfig {
                        name: ft.name.clone(),
                        url: url.clone(),
                        destination: destination.clone(),
                        strip_components: ft.strip_components,
                        os: ft.os.clone(),
                        arch: ft.arch.clone(),
                        sha256: ft.sha256.clone(),
                        mirror_destination: ft.mirror_destination.clone(),
                        environment: ft.environment.clone(),
                        post_install_commands: ft.post_install_commands.clone(),
                    });
                }
            } else {
                return Err(format!(
                    "Fragment '{}': toolchain entry must have a 'name' or 'url' field \
                     for identification",
                    fragment_name
                )
                .into());
            }
        }
    }

    if let Some(ref fragment_vars) = fragment.variables {
        let vars = config.variables.get_or_insert_with(Default::default);
        for (key, value) in fragment_vars {
            vars.insert(key.clone(), value.clone());
        }
    }

    if let Some(ref fragment_copy_files) = fragment.copy_files {
        let copy_files = config.copy_files.get_or_insert_with(Vec::new);
        copy_files.extend(fragment_copy_files.clone());
    }

    if let Some(ref fragment_install) = fragment.install {
        let installs = config.install.get_or_insert_with(Vec::new);
        for fi in fragment_install {
            if let Some(existing) = installs.iter_mut().find(|i| i.name == fi.name) {
                merge_install(existing, fi);
            } else {
                installs.push(fi.clone());
            }
        }
    }

    // --- Scalar replacements ---

    if let Some(ref mirror) = fragment.mirror {
        config.mirror = mirror.clone();
    }

    if let Some(ref build_folder) = fragment.build_folder {
        config.build_folder = Some(build_folder.clone());
    }

    // --- Global target replacements ---

    if fragment.build.is_some() {
        config.build = fragment.build.clone();
    }
    if fragment.test.is_some() {
        config.test = fragment.test.clone();
    }
    if fragment.clean.is_some() {
        config.clean = fragment.clean.clone();
    }
    if fragment.flash.is_some() {
        config.flash = fragment.flash.clone();
    }
    if fragment.envsetup.is_some() {
        config.envsetup = fragment.envsetup.clone();
    }

    // --- Makefile include (append) ---

    if let Some(ref fragment_mki) = fragment.makefile_include {
        merge_makefile_include(config, fragment_mki);
    }

    Ok(())
}

/// Apply multiple fragments in order. Later fragments override earlier ones.
pub fn apply_fragments(
    config: &mut SdkConfig,
    fragments: &[Fragment],
) -> Result<(), Box<dyn std::error::Error>> {
    for (i, fragment) in fragments.iter().enumerate() {
        let name = fragment
            .fragment
            .as_ref()
            .and_then(|m| m.name.as_deref())
            .unwrap_or("<unnamed>");
        messages::verbose(&format!("Applying fragment {}: '{}'", i + 1, name));
        apply_fragment(config, fragment)?;
    }
    Ok(())
}

/// Discover fragment files in a directory.
/// Returns paths to `*.fragment.yml` files, sorted alphabetically.
pub fn discover_fragments_in_dir(dir: &Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(vec![]);
    }

    let mut fragments: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
                .unwrap_or(false)
        })
        .collect();

    // Alphabetical order determines priority: later files override earlier
    fragments.sort();
    Ok(fragments)
}

/// Deep-merge a fragment git entry into an existing GitConfig.
/// Only fields that are `Some` in the fragment override the base.
fn merge_git(base: &mut GitConfig, overlay: &FragmentGitConfig) {
    if let Some(ref url) = overlay.url {
        base.url = url.clone();
    }
    if let Some(ref commit) = overlay.commit {
        base.commit = commit.clone();
    }
    if overlay.build_depends_on.is_some() {
        base.build_depends_on = overlay.build_depends_on.clone();
    }
    if overlay.git_depends_on.is_some() {
        base.git_depends_on = overlay.git_depends_on.clone();
    }
    if overlay.build.is_some() {
        base.build = overlay.build.clone();
    }
    if overlay.documentation_dir.is_some() {
        base.documentation_dir = overlay.documentation_dir.clone();
    }
}

/// Deep-merge a fragment toolchain entry into an existing ToolchainConfig.
/// Only fields that are `Some` in the fragment override the base.
fn merge_toolchain(base: &mut ToolchainConfig, overlay: &FragmentToolchainConfig) {
    if let Some(ref name) = overlay.name {
        base.name = Some(name.clone());
    }
    if let Some(ref url) = overlay.url {
        base.url = url.clone();
    }
    if let Some(ref destination) = overlay.destination {
        base.destination = destination.clone();
    }
    if overlay.strip_components.is_some() {
        base.strip_components = overlay.strip_components;
    }
    if overlay.os.is_some() {
        base.os = overlay.os.clone();
    }
    if overlay.arch.is_some() {
        base.arch = overlay.arch.clone();
    }
    if overlay.sha256.is_some() {
        base.sha256 = overlay.sha256.clone();
    }
    if overlay.mirror_destination.is_some() {
        base.mirror_destination = overlay.mirror_destination.clone();
    }
    if overlay.environment.is_some() {
        base.environment = overlay.environment.clone();
    }
    if overlay.post_install_commands.is_some() {
        base.post_install_commands = overlay.post_install_commands.clone();
    }
}

/// Deep-merge a fragment install entry into an existing InstallConfig.
/// Only fields that are `Some` in the fragment override the base.
fn merge_install(base: &mut InstallConfig, overlay: &InstallConfig) {
    if overlay.depends_on.is_some() {
        base.depends_on = overlay.depends_on.clone();
    }
    if overlay.sentinel.is_some() {
        base.sentinel = overlay.sentinel.clone();
    }
    if overlay.commands.is_some() {
        base.commands = overlay.commands.clone();
    }
}

/// Merge makefile_include from a fragment into the base config (append semantics).
fn merge_makefile_include(config: &mut SdkConfig, fragment_mki: &MakefileInclude) {
    let fragment_files = fragment_mki.files();
    let fragment_exclude = fragment_mki.exclude();

    match config.makefile_include {
        Some(ref mut existing) => {
            // Convert to structured form if currently legacy, then append
            let structured = match existing {
                MakefileInclude::Legacy(files) => {
                    let mut config = MakefileIncludeConfig {
                        files: files.clone(),
                        exclude: vec![],
                    };
                    config.files.extend(fragment_files.iter().cloned());
                    config.exclude.extend(fragment_exclude.iter().cloned());
                    config
                }
                MakefileInclude::Structured(ref mut config) => {
                    config.files.extend(fragment_files.iter().cloned());
                    config.exclude.extend(fragment_exclude.iter().cloned());
                    return;
                }
            };
            *existing = MakefileInclude::Structured(structured);
        }
        None => {
            if !fragment_files.is_empty() || !fragment_exclude.is_empty() {
                config.makefile_include =
                    Some(MakefileInclude::Structured(MakefileIncludeConfig {
                        files: fragment_files.to_vec(),
                        exclude: fragment_exclude.to_vec(),
                    }));
            }
        }
    }
}

/// Validate a merged SdkConfig for consistency issues.
/// Returns a list of warnings (non-fatal) and errors (fatal).
pub fn validate_merged_config(config: &SdkConfig) -> Vec<String> {
    let mut warnings = Vec::new();

    // Check for dangling build_depends_on references
    let git_names: std::collections::HashSet<&str> =
        config.gits.iter().map(|g| g.name.as_str()).collect();

    for git in &config.gits {
        if let Some(ref deps) = git.build_depends_on {
            for dep in deps {
                if !git_names.contains(dep.as_str()) {
                    warnings.push(format!(
                        "Git '{}' has build_depends_on '{}' which does not exist",
                        git.name, dep
                    ));
                }
            }
        }
        if let Some(ref deps) = git.git_depends_on {
            for dep in deps {
                if !git_names.contains(dep.as_str()) {
                    warnings.push(format!(
                        "Git '{}' has git_depends_on '{}' which does not exist",
                        git.name, dep
                    ));
                }
            }
        }
    }

    // Check for dangling install depends_on references
    if let Some(ref installs) = config.install {
        let install_names: std::collections::HashSet<&str> =
            installs.iter().map(|i| i.name.as_str()).collect();
        for install in installs {
            if let Some(ref deps) = install.depends_on {
                for dep in deps {
                    if !install_names.contains(dep.as_str()) {
                        warnings.push(format!(
                            "Install '{}' has depends_on '{}' which does not exist",
                            install.name, dep
                        ));
                    }
                }
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{load_config, load_fragment};
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_base_config() -> SdkConfig {
        let yaml = r#"
mirror: /tmp/mirror

gits:
  - name: kernel
    url: https://github.com/torvalds/linux.git
    commit: v6.0
    build:
      - make -j8
  - name: u-boot
    url: https://github.com/u-boot/u-boot.git
    commit: v2023.01
    build_depends_on:
      - kernel

toolchains:
  - name: arm-gcc
    url: https://example.com/arm-gcc-12.tar.xz
    destination: /opt/toolchains/arm-gcc
    os: linux
    arch: x86_64

variables:
  CROSS_COMPILE: aarch64-linux-gnu-
  ARCH: arm64

copy_files:
  - source: shared/setup.sh
    dest: scripts/setup.sh

install:
  - name: ninja
    commands:
      - apt-get install ninja-build
  - name: cmake
    depends_on:
      - ninja
    commands:
      - apt-get install cmake

build: make all
test: make test
clean: make clean
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_apply_fragment_modify_git_commit() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
gits:
  - name: kernel
    commit: v6.5
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let kernel = config.gits.iter().find(|g| g.name == "kernel").unwrap();
        assert_eq!(kernel.commit, "v6.5");
        // URL and build should be preserved
        assert_eq!(kernel.url, "https://github.com/torvalds/linux.git");
        assert_eq!(
            kernel.build.as_ref().unwrap(),
            &vec!["make -j8".to_string()]
        );
    }

    #[test]
    fn test_apply_fragment_add_new_git() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
gits:
  - name: my-app
    url: https://github.com/me/app.git
    commit: main
    build:
      - make app
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        assert_eq!(config.gits.len(), 3);
        let app = config.gits.iter().find(|g| g.name == "my-app").unwrap();
        assert_eq!(app.url, "https://github.com/me/app.git");
        assert_eq!(app.commit, "main");
    }

    #[test]
    fn test_apply_fragment_new_git_missing_url_errors() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
gits:
  - name: my-app
    commit: main
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        let result = apply_fragment(&mut config, &fragment);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires 'url'"));
    }

    #[test]
    fn test_apply_fragment_remove_git() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
remove_gits:
  - u-boot
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        assert_eq!(config.gits.len(), 1);
        assert_eq!(config.gits[0].name, "kernel");
    }

    #[test]
    fn test_apply_fragment_modify_toolchain() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
toolchains:
  - name: arm-gcc
    url: https://example.com/arm-gcc-13.tar.xz
    sha256: abc123
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let tc = &config.toolchains.as_ref().unwrap()[0];
        assert_eq!(tc.url, "https://example.com/arm-gcc-13.tar.xz");
        assert_eq!(tc.sha256.as_ref().unwrap(), "abc123");
        // Destination preserved
        assert_eq!(tc.destination, "/opt/toolchains/arm-gcc");
    }

    #[test]
    fn test_apply_fragment_remove_toolchain() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
remove_toolchains:
  - arm-gcc
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        assert!(config.toolchains.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_apply_fragment_variables_merge() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
variables:
  CROSS_COMPILE: arm-none-eabi-
  NEW_VAR: hello
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let vars = config.variables.as_ref().unwrap();
        assert_eq!(vars["CROSS_COMPILE"], "arm-none-eabi-");
        assert_eq!(vars["NEW_VAR"], "hello");
        // Existing untouched variable preserved
        assert_eq!(vars["ARCH"], "arm64");
    }

    #[test]
    fn test_apply_fragment_replace_build_target() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
build: make custom-build
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        assert_eq!(
            config.build.as_ref().unwrap().commands(),
            &["make custom-build"]
        );
    }

    #[test]
    fn test_apply_fragment_copy_files_append() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
copy_files:
  - source: my/file.sh
    dest: scripts/file.sh
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let cf = config.copy_files.as_ref().unwrap();
        assert_eq!(cf.len(), 2);
        assert_eq!(cf[1].dest, "scripts/file.sh");
    }

    #[test]
    fn test_apply_fragment_remove_copy_files() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
remove_copy_files:
  - dest: scripts/setup.sh
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        assert!(config.copy_files.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_apply_fragment_install_modify() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
install:
  - name: ninja
    commands:
      - pip install ninja
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let ninja = config
            .install
            .as_ref()
            .unwrap()
            .iter()
            .find(|i| i.name == "ninja")
            .unwrap();
        assert_eq!(
            ninja.commands.as_ref().unwrap(),
            &vec!["pip install ninja".to_string()]
        );
    }

    #[test]
    fn test_apply_fragment_remove_install() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
remove_install:
  - cmake
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let installs = config.install.as_ref().unwrap();
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].name, "ninja");
    }

    #[test]
    fn test_apply_fragment_mirror_override() {
        let mut config = create_base_config();
        let fragment_yaml = r#"
mirror: /custom/mirror
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        assert_eq!(config.mirror, std::path::PathBuf::from("/custom/mirror"));
    }

    #[test]
    fn test_apply_fragments_ordering() {
        let mut config = create_base_config();

        let frag1_yaml = r#"
gits:
  - name: kernel
    commit: v6.3
"#;
        let frag2_yaml = r#"
gits:
  - name: kernel
    commit: v6.5
"#;
        let frag1: Fragment = serde_yaml::from_str(frag1_yaml).unwrap();
        let frag2: Fragment = serde_yaml::from_str(frag2_yaml).unwrap();

        apply_fragments(&mut config, &[frag1, frag2]).unwrap();

        let kernel = config.gits.iter().find(|g| g.name == "kernel").unwrap();
        // Last fragment wins
        assert_eq!(kernel.commit, "v6.5");
    }

    #[test]
    fn test_validate_merged_config_dangling_deps() {
        let mut config = create_base_config();
        // Remove kernel but u-boot depends on it
        config.gits.retain(|g| g.name != "kernel");

        let warnings = validate_merged_config(&config);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("build_depends_on 'kernel'"));
    }

    #[test]
    fn test_discover_fragments_in_dir() {
        let dir = tempdir().unwrap();

        // Create some fragment files and a non-fragment file
        File::create(dir.path().join("01-add-repo.yml")).unwrap();
        File::create(dir.path().join("02-override.yml")).unwrap();
        File::create(dir.path().join("03-extra.fragment.yml")).unwrap();
        File::create(dir.path().join("readme.md")).unwrap();

        let found = discover_fragments_in_dir(dir.path()).unwrap();
        // All .yml files are discovered, non-yml files ignored
        assert_eq!(found.len(), 3);
        assert!(found[0].file_name().unwrap().to_str().unwrap() == "01-add-repo.yml");
        assert!(found[1].file_name().unwrap().to_str().unwrap() == "02-override.yml");
        assert!(found[2].file_name().unwrap().to_str().unwrap() == "03-extra.fragment.yml");
    }

    #[test]
    fn test_discover_fragments_nonexistent_dir() {
        let result = discover_fragments_in_dir(Path::new("/nonexistent/path"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_load_and_apply_fragment_from_file() {
        let dir = tempdir().unwrap();

        // Write a base config
        let base_path = dir.path().join("sdk.yml");
        let mut base_file = File::create(&base_path).unwrap();
        write!(
            base_file,
            r#"
mirror: /tmp/mirror
gits:
  - name: repo1
    url: https://example.com/repo1.git
    commit: main
"#
        )
        .unwrap();

        // Write a fragment
        let frag_path = dir.path().join("custom.fragment.yml");
        let mut frag_file = File::create(&frag_path).unwrap();
        write!(
            frag_file,
            r#"
fragment:
  name: custom-override
  description: "Override repo1 commit"

gits:
  - name: repo1
    commit: v2.0
"#
        )
        .unwrap();

        let mut config = load_config(&base_path).unwrap();
        let fragment = load_fragment(&frag_path).unwrap();

        assert_eq!(
            fragment.fragment.as_ref().unwrap().name.as_deref(),
            Some("custom-override")
        );

        apply_fragment(&mut config, &fragment).unwrap();
        assert_eq!(config.gits[0].commit, "v2.0");
        // URL preserved
        assert_eq!(config.gits[0].url, "https://example.com/repo1.git");
    }

    #[test]
    fn test_apply_fragment_makefile_include_append() {
        let mut config = create_base_config();
        // Add initial makefile_include
        config.makefile_include = Some(MakefileInclude::Structured(MakefileIncludeConfig {
            files: vec!["include base.mk".to_string()],
            exclude: vec!["excluded-repo".to_string()],
        }));

        let fragment_yaml = r#"
makefile_include:
  files:
    - "include custom.mk"
  exclude:
    - another-excluded
"#;
        let fragment: Fragment = serde_yaml::from_str(fragment_yaml).unwrap();
        apply_fragment(&mut config, &fragment).unwrap();

        let mki = config.makefile_include.as_ref().unwrap();
        assert_eq!(mki.files().len(), 2);
        assert_eq!(mki.files()[1], "include custom.mk");
        assert_eq!(mki.exclude().len(), 2);
        assert_eq!(mki.exclude()[1], "another-excluded");
    }
}
