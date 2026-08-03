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

//! Data model and merge engine for the `extends:`/`overlay.yml` feature.
//!
//! A target's sdk.yml may declare `extends: <base-target>@<version>` to be
//! based on another target. The actual add/remove/modify diff against that
//! base lives in a sibling `overlay.yml` file (not in sdk.yml itself), using
//! the shapes defined in this module. This module only implements the pure,
//! in-memory merge logic; resolving `extends:` against a manifest source
//! (network or local disk) lives in `init_cmd.rs`.
//!
//! Merge order for every list section is fixed: **remove -> modify -> add**.
//! Every remove/modify reference must resolve to an existing entry, and
//! every add must NOT already exist — both are hard errors, not warnings,
//! to avoid silently masking manifest authoring mistakes.

use crate::config::{
    deserialize_string_or_vec, CopyFileConfig, GitConfig, InstallConfig, SdkConfig, SdkConfigCore,
    ToolchainConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A patch applied to an existing `gits:` entry identified by `name`. Only
/// fields explicitly set in overlay.yml are overridden; all other fields of
/// the base entry are preserved unchanged. `name` itself is the identity key
/// and is never modified by a patch (renaming a git via overlay is not
/// supported).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GitPatch {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default, alias = "depends_on")]
    pub build_depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub git_depends_on: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub build: Option<Vec<String>>,
    #[serde(default)]
    pub documentation_dir: Option<String>,
    #[serde(
        rename = "python-deps",
        default,
        deserialize_with = "deserialize_string_or_vec"
    )]
    pub python_deps: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub group: Option<Vec<String>>,
}

/// A patch applied to an existing `toolchains:` entry, identified by its
/// effective name (see `ToolchainConfig::get_name()`).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolchainPatch {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub strip_components: Option<u32>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub mirror_destination: Option<String>,
    #[serde(default)]
    pub environment: Option<HashMap<String, String>>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub post_install_commands: Option<Vec<String>>,
}

/// A patch applied to an existing `install:` entry, identified by `name`.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InstallPatch {
    pub name: String,
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub sentinel: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub commands: Option<Vec<String>>,
}

/// A patch applied to an existing `copy_files:` entry, identified by `dest`
/// (the workspace-relative destination path, which is already unique per
/// entry).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CopyFilePatch {
    pub dest: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub cache: Option<bool>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub post_data: Option<String>,
    #[serde(default)]
    pub symlink: Option<bool>,
}

/// Add/remove/modify diff for the `gits:` section of overlay.yml.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GitsOverlay {
    #[serde(default)]
    pub add: Vec<GitConfig>,
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub modify: Vec<GitPatch>,
}

/// Add/remove/modify diff for the `toolchains:` section of overlay.yml.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ToolchainsOverlay {
    #[serde(default)]
    pub add: Vec<ToolchainConfig>,
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub modify: Vec<ToolchainPatch>,
}

/// Add/remove/modify diff for the `install:` section of overlay.yml.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct InstallOverlay {
    #[serde(default)]
    pub add: Vec<InstallConfig>,
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub modify: Vec<InstallPatch>,
}

/// Add/remove/modify diff for the `copy_files:` section of overlay.yml.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CopyFilesOverlay {
    #[serde(default)]
    pub add: Vec<CopyFileConfig>,
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub modify: Vec<CopyFilePatch>,
}

/// Add/override/remove diff for the `variables:` section of overlay.yml.
/// Unlike the list sections, `set` both adds new keys and overrides existing
/// ones (a key-value map has no meaningful add/modify distinction); `remove`
/// still requires the key to exist in the base.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct VariablesOverlay {
    #[serde(default)]
    pub set: HashMap<String, String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Top-level `overlay.yml` contents: the add/remove/modify diff applied to a
/// base target's resolved configuration. All sections are optional; an
/// entirely empty (or absent) overlay.yml is valid and is a no-op merge.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct OverlayConfig {
    #[serde(default)]
    pub gits: Option<GitsOverlay>,
    #[serde(default)]
    pub toolchains: Option<ToolchainsOverlay>,
    #[serde(default)]
    pub install: Option<InstallOverlay>,
    #[serde(default)]
    pub copy_files: Option<CopyFilesOverlay>,
    #[serde(default)]
    pub variables: Option<VariablesOverlay>,
}

/// Load `overlay.yml` from disk. Callers should check the file's existence
/// first and use `OverlayConfig::default()` when it is absent (a missing
/// overlay.yml is valid, e.g. for a target that only re-tags its base).
pub fn load_overlay_config<P: AsRef<Path>>(
    path: P,
) -> Result<OverlayConfig, Box<dyn std::error::Error>> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Cannot read overlay file {}: {}", path.display(), e))?;
    let overlay: OverlayConfig = noyalib::from_str(&content)
        .map_err(|e| format!("Overlay validation error in {}: {}", path.display(), e))?;
    Ok(overlay)
}

/// Find the index of the item whose `key_of` value matches `key`.
fn position_by<T>(items: &[T], key: &str, key_of: impl Fn(&T) -> String) -> Option<usize> {
    items.iter().position(|item| key_of(item) == key)
}

fn apply_git_patch(target: &mut GitConfig, patch: &GitPatch) {
    if let Some(v) = &patch.url {
        target.url = v.clone();
    }
    if let Some(v) = &patch.commit {
        target.commit = v.clone();
    }
    if patch.build_depends_on.is_some() {
        target.build_depends_on = patch.build_depends_on.clone();
    }
    if patch.git_depends_on.is_some() {
        target.git_depends_on = patch.git_depends_on.clone();
    }
    if patch.build.is_some() {
        target.build = patch.build.clone();
    }
    if patch.documentation_dir.is_some() {
        target.documentation_dir = patch.documentation_dir.clone();
    }
    if patch.python_deps.is_some() {
        target.python_deps = patch.python_deps.clone();
    }
    if patch.group.is_some() {
        target.group = patch.group.clone();
    }
}

fn apply_toolchain_patch(target: &mut ToolchainConfig, patch: &ToolchainPatch) {
    if let Some(v) = &patch.url {
        target.url = v.clone();
    }
    if let Some(v) = &patch.destination {
        target.destination = v.clone();
    }
    if patch.strip_components.is_some() {
        target.strip_components = patch.strip_components;
    }
    if patch.os.is_some() {
        target.os = patch.os.clone();
    }
    if patch.arch.is_some() {
        target.arch = patch.arch.clone();
    }
    if patch.sha256.is_some() {
        target.sha256 = patch.sha256.clone();
    }
    if patch.mirror_destination.is_some() {
        target.mirror_destination = patch.mirror_destination.clone();
    }
    if patch.environment.is_some() {
        target.environment = patch.environment.clone();
    }
    if patch.post_install_commands.is_some() {
        target.post_install_commands = patch.post_install_commands.clone();
    }
}

fn apply_install_patch(target: &mut InstallConfig, patch: &InstallPatch) {
    if patch.depends_on.is_some() {
        target.depends_on = patch.depends_on.clone();
    }
    if patch.sentinel.is_some() {
        target.sentinel = patch.sentinel.clone();
    }
    if patch.commands.is_some() {
        target.commands = patch.commands.clone();
    }
}

fn apply_copy_file_patch(target: &mut CopyFileConfig, patch: &CopyFilePatch) {
    if let Some(v) = &patch.source {
        target.source = v.clone();
    }
    if patch.cache.is_some() {
        target.cache = patch.cache;
    }
    if patch.sha256.is_some() {
        target.sha256 = patch.sha256.clone();
    }
    if patch.post_data.is_some() {
        target.post_data = patch.post_data.clone();
    }
    if patch.symlink.is_some() {
        target.symlink = patch.symlink;
    }
}

/// Merge the `gits:` section: base list + overlay's remove/modify/add.
pub fn merge_gits(
    base: Vec<GitConfig>,
    overlay: Option<&GitsOverlay>,
) -> Result<Vec<GitConfig>, String> {
    let mut result = base;
    let Some(overlay) = overlay else {
        return Ok(result);
    };

    for name in &overlay.remove {
        match position_by(&result, name, |g| g.name.clone()) {
            Some(idx) => {
                result.remove(idx);
            }
            None => {
                return Err(format!(
                    "overlay.yml: cannot remove git '{}': not found in base",
                    name
                ))
            }
        }
    }

    for patch in &overlay.modify {
        match position_by(&result, &patch.name, |g| g.name.clone()) {
            Some(idx) => apply_git_patch(&mut result[idx], patch),
            None => {
                return Err(format!(
                    "overlay.yml: cannot modify git '{}': not found (it may have \
                     been removed by this overlay, or never existed in the base)",
                    patch.name
                ))
            }
        }
    }

    for git in &overlay.add {
        if position_by(&result, &git.name, |g| g.name.clone()).is_some() {
            return Err(format!(
                "overlay.yml: cannot add git '{}': already exists in base; use modify: instead",
                git.name
            ));
        }
        result.push(git.clone());
    }

    Ok(result)
}

/// Merge the `toolchains:` section: base list + overlay's remove/modify/add.
pub fn merge_toolchains(
    base: Option<Vec<ToolchainConfig>>,
    overlay: Option<&ToolchainsOverlay>,
) -> Result<Option<Vec<ToolchainConfig>>, String> {
    let mut result = base.unwrap_or_default();
    let Some(overlay) = overlay else {
        return Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        });
    };

    for name in &overlay.remove {
        match position_by(&result, name, |t| t.get_name()) {
            Some(idx) => {
                result.remove(idx);
            }
            None => {
                return Err(format!(
                    "overlay.yml: cannot remove toolchain '{}': not found in base",
                    name
                ))
            }
        }
    }

    for patch in &overlay.modify {
        match position_by(&result, &patch.name, |t| t.get_name()) {
            Some(idx) => apply_toolchain_patch(&mut result[idx], patch),
            None => {
                return Err(format!(
                    "overlay.yml: cannot modify toolchain '{}': not found (it may \
                     have been removed by this overlay, or never existed in the base)",
                    patch.name
                ))
            }
        }
    }

    for toolchain in &overlay.add {
        let name = toolchain.get_name();
        if position_by(&result, &name, |t| t.get_name()).is_some() {
            return Err(format!(
                "overlay.yml: cannot add toolchain '{}': already exists in base; use modify: instead",
                name
            ));
        }
        result.push(toolchain.clone());
    }

    Ok(if result.is_empty() {
        None
    } else {
        Some(result)
    })
}

/// Merge the `install:` section: base list + overlay's remove/modify/add.
pub fn merge_install(
    base: Option<Vec<InstallConfig>>,
    overlay: Option<&InstallOverlay>,
) -> Result<Option<Vec<InstallConfig>>, String> {
    let mut result = base.unwrap_or_default();
    let Some(overlay) = overlay else {
        return Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        });
    };

    for name in &overlay.remove {
        match position_by(&result, name, |i| i.name.clone()) {
            Some(idx) => {
                result.remove(idx);
            }
            None => {
                return Err(format!(
                    "overlay.yml: cannot remove install target '{}': not found in base",
                    name
                ))
            }
        }
    }

    for patch in &overlay.modify {
        match position_by(&result, &patch.name, |i| i.name.clone()) {
            Some(idx) => apply_install_patch(&mut result[idx], patch),
            None => {
                return Err(format!(
                    "overlay.yml: cannot modify install target '{}': not found (it \
                     may have been removed by this overlay, or never existed in the base)",
                    patch.name
                ))
            }
        }
    }

    for install in &overlay.add {
        if position_by(&result, &install.name, |i| i.name.clone()).is_some() {
            return Err(format!(
                "overlay.yml: cannot add install target '{}': already exists in base; use modify: instead",
                install.name
            ));
        }
        result.push(install.clone());
    }

    Ok(if result.is_empty() {
        None
    } else {
        Some(result)
    })
}

/// Merge the `copy_files:` section: base list + overlay's remove/modify/add,
/// keyed by `dest`.
pub fn merge_copy_files(
    base: Option<Vec<CopyFileConfig>>,
    overlay: Option<&CopyFilesOverlay>,
) -> Result<Option<Vec<CopyFileConfig>>, String> {
    let mut result = base.unwrap_or_default();
    let Some(overlay) = overlay else {
        return Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        });
    };

    for dest in &overlay.remove {
        match position_by(&result, dest, |c| c.dest.clone()) {
            Some(idx) => {
                result.remove(idx);
            }
            None => {
                return Err(format!(
                    "overlay.yml: cannot remove copy_files entry '{}': not found in base",
                    dest
                ))
            }
        }
    }

    for patch in &overlay.modify {
        match position_by(&result, &patch.dest, |c| c.dest.clone()) {
            Some(idx) => apply_copy_file_patch(&mut result[idx], patch),
            None => {
                return Err(format!(
                    "overlay.yml: cannot modify copy_files entry '{}': not found (it \
                     may have been removed by this overlay, or never existed in the base)",
                    patch.dest
                ))
            }
        }
    }

    for entry in &overlay.add {
        if position_by(&result, &entry.dest, |c| c.dest.clone()).is_some() {
            return Err(format!(
                "overlay.yml: cannot add copy_files entry '{}': already exists in base; use modify: instead",
                entry.dest
            ));
        }
        result.push(entry.clone());
    }

    Ok(if result.is_empty() {
        None
    } else {
        Some(result)
    })
}

/// Merge the `variables:` section: `set` upserts keys, `remove` deletes an
/// existing key (erroring if it doesn't exist).
pub fn merge_variables(
    base: Option<HashMap<String, String>>,
    overlay: Option<&VariablesOverlay>,
) -> Result<Option<HashMap<String, String>>, String> {
    let mut result = base.unwrap_or_default();
    if let Some(overlay) = overlay {
        for key in &overlay.remove {
            if result.remove(key).is_none() {
                return Err(format!(
                    "overlay.yml: cannot remove variable '{}': not found in base",
                    key
                ));
            }
        }
        for (k, v) in &overlay.set {
            result.insert(k.clone(), v.clone());
        }
    }
    Ok(if result.is_empty() {
        None
    } else {
        Some(result)
    })
}

/// Merge a base target's resolved `SdkConfig` with a derived target's own
/// `SdkConfig` (the one declaring `extends:`) and that derived target's
/// `overlay.yml`, producing the effective, in-memory `SdkConfig` for this
/// level of the chain.
///
/// List sections (gits/toolchains/install/copy_files) in `derived` must be
/// empty — they are only allowed via `overlay.yml` when `extends:` is set.
/// Scalar sections (build/test/clean/flash/envsetup/build_folder/
/// makefile_include/direnv/phases) in `derived` override the corresponding
/// value inherited from `base` when present.
pub fn apply_overlay(
    base: SdkConfig,
    derived: SdkConfig,
    overlay: &OverlayConfig,
) -> Result<SdkConfig, String> {
    if !derived.gits.is_empty() {
        return Err(
            "sdk.yml: 'gits:' must be empty when 'extends:' is set; add/remove/modify \
             gits via overlay.yml instead"
                .to_string(),
        );
    }
    if derived.toolchains.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(
            "sdk.yml: 'toolchains:' must be empty when 'extends:' is set; use overlay.yml instead"
                .to_string(),
        );
    }
    if derived.install.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(
            "sdk.yml: 'install:' must be empty when 'extends:' is set; use overlay.yml instead"
                .to_string(),
        );
    }
    if derived.copy_files.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(
            "sdk.yml: 'copy_files:' must be empty when 'extends:' is set; use overlay.yml instead"
                .to_string(),
        );
    }

    let gits = merge_gits(base.gits, overlay.gits.as_ref())?;
    let toolchains = merge_toolchains(base.toolchains, overlay.toolchains.as_ref())?;
    let install = merge_install(base.install, overlay.install.as_ref())?;
    let copy_files = merge_copy_files(base.copy_files, overlay.copy_files.as_ref())?;
    let variables = merge_variables(base.variables, overlay.variables.as_ref())?;

    Ok(SdkConfig {
        gits,
        toolchains,
        copy_files,
        install,
        makefile_include: derived.makefile_include.or(base.makefile_include),
        extends: None,
        build_folder: derived.build_folder.or(base.build_folder),
        envsetup: derived.envsetup.or(base.envsetup),
        test: derived.test.or(base.test),
        clean: derived.clean.or(base.clean),
        build: derived.build.or(base.build),
        flash: derived.flash.or(base.flash),
        variables,
        phases: derived.phases.or(base.phases),
        direnv: derived.direnv.or(base.direnv),
    })
}

/// Validate that every `build_depends_on`/`git_depends_on` (gits) and
/// `depends_on` (install) reference resolves to an existing entry in the
/// final, fully-merged configuration. Collects every dangling reference into
/// a single error rather than failing on the first one, so a manifest author
/// can fix them all in one pass.
///
/// `build_depends_on` is emitted verbatim as a Makefile prerequisite (see
/// `makefile::add_makefile_target`), so besides other git names it may also
/// legitimately reference a phase target (`sdk-envsetup`, `sdk-build`, ...)
/// or an install target (`install-<name>`); both are accepted here.
/// `git_depends_on` (clone ordering) only makes sense against other gits.
pub fn validate_dependencies(config: &SdkConfig) -> Result<(), String> {
    let git_names: std::collections::HashSet<&str> =
        config.gits.iter().map(|g| g.name.as_str()).collect();
    let install_names: std::collections::HashSet<String> = config
        .install
        .as_ref()
        .map(|installs| installs.iter().map(|i| i.name.clone()).collect())
        .unwrap_or_default();

    let mut valid_build_targets: std::collections::HashSet<String> =
        git_names.iter().map(|n| n.to_string()).collect();
    valid_build_targets.extend(config.phases().iter().map(|p| format!("sdk-{}", p)));
    valid_build_targets.extend(install_names.iter().map(|n| format!("install-{}", n)));

    let mut errors = Vec::new();

    for git in &config.gits {
        if let Some(deps) = &git.build_depends_on {
            for dep in deps {
                if !valid_build_targets.contains(dep.as_str()) {
                    errors.push(format!(
                        "git '{}': build_depends_on references unknown target '{}' \
                         (not a git, phase, or install target)",
                        git.name, dep
                    ));
                }
            }
        }
        if let Some(deps) = &git.git_depends_on {
            for dep in deps {
                if !git_names.contains(dep.as_str()) {
                    errors.push(format!(
                        "git '{}': git_depends_on references unknown git '{}'",
                        git.name, dep
                    ));
                }
            }
        }
    }

    if let Some(installs) = &config.install {
        for install in installs {
            if let Some(deps) = &install.depends_on {
                for dep in deps {
                    if !install_names.contains(dep.as_str()) {
                        errors.push(format!(
                            "install '{}': depends_on references unknown install target '{}'",
                            install.name, dep
                        ));
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Dependency validation failed after resolving extends/overlay:\n  - {}",
            errors.join("\n  - ")
        ))
    }
}

/// Names of the entries "owned" by an overlay.yml -- i.e. added or modified
/// by this target, as opposed to inherited unmodified from the base target.
/// Used by commands that must scope their effect (or their writes back to
/// disk) to only what a derived target's own manifest actually controls:
/// `cim release` (tagging/freezing commits) and `cim utils hash-toolchains`/
/// `hash-copy-files` (writing computed hashes back into a manifest file).
#[derive(Debug, Clone, Default)]
pub struct OwnedEntries {
    pub gits: std::collections::HashSet<String>,
    pub toolchains: std::collections::HashSet<String>,
    pub install: std::collections::HashSet<String>,
    pub copy_files: std::collections::HashSet<String>,
}

/// Compute the set of entry identities (`name`, or `dest` for copy_files)
/// added or modified by `overlay`, per section.
pub fn compute_owned_entries(overlay: &OverlayConfig) -> OwnedEntries {
    let mut owned = OwnedEntries::default();

    if let Some(gits) = &overlay.gits {
        owned.gits.extend(gits.add.iter().map(|g| g.name.clone()));
        owned
            .gits
            .extend(gits.modify.iter().map(|p| p.name.clone()));
    }
    if let Some(toolchains) = &overlay.toolchains {
        owned
            .toolchains
            .extend(toolchains.add.iter().map(|t| t.get_name()));
        owned
            .toolchains
            .extend(toolchains.modify.iter().map(|p| p.name.clone()));
    }
    if let Some(install) = &overlay.install {
        owned
            .install
            .extend(install.add.iter().map(|i| i.name.clone()));
        owned
            .install
            .extend(install.modify.iter().map(|p| p.name.clone()));
    }
    if let Some(copy_files) = &overlay.copy_files {
        owned
            .copy_files
            .extend(copy_files.add.iter().map(|c| c.dest.clone()));
        owned
            .copy_files
            .extend(copy_files.modify.iter().map(|p| p.dest.clone()));
    }

    owned
}
