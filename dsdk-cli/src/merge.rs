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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::config::{
    OsConfig, OsDependencies, PythonDependencies, PythonProfile, SdkConfig, ToolchainConfig,
};

/// A conflict detected during target merge.
#[derive(Debug, Clone)]
pub struct MergeConflict {
    /// Which section contains the conflict (e.g., "gits", "toolchains")
    pub section: String,
    /// The conflicting key/name
    pub key: String,
    /// Which targets define this key
    pub sources: Vec<String>,
}

impl std::fmt::Display for MergeConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: '{}' defined in targets: {}",
            self.section,
            self.key,
            self.sources.join(", ")
        )
    }
}

/// Result of a merge operation.
#[derive(Debug)]
pub struct MergeResult {
    /// The merged configuration (only valid if conflicts is empty)
    pub config: SdkConfig,
    /// Any conflicts found during merge
    pub conflicts: Vec<MergeConflict>,
    /// Informational notes about the merge
    pub notes: Vec<String>,
}

/// Merge multiple SDK configurations from different targets into one.
///
/// The merge uses an error-on-conflict strategy:
/// - List items (gits, toolchains, install, copy_files) are combined if
///   they have unique keys; conflicts (same key in multiple targets) are
///   reported as errors.
/// - Variables are combined; same key with different values is a conflict.
/// - Global targets (build, test, clean, flash, envsetup) are omitted
///   from the merged output — users must provide them via a fragment.
/// - mirror: taken from the first target, or overridden via `mirror_override`.
/// - makefile_include: files and exclude lists are concatenated.
/// - build_folder: taken from the first target that defines it.
///
/// # Arguments
///
/// * `targets` - Pairs of (target_name, SdkConfig)
/// * `mirror_override` - Optional mirror path override
pub fn merge_targets(
    targets: &[(&str, &SdkConfig)],
    mirror_override: Option<&std::path::Path>,
) -> MergeResult {
    let mut conflicts = Vec::new();
    let mut notes = Vec::new();

    if targets.is_empty() {
        return MergeResult {
            config: SdkConfig {
                mirror: std::path::PathBuf::new(),
                gits: vec![],
                toolchains: None,
                copy_files: None,
                install: None,
                makefile_include: None,
                build_folder: None,
                envsetup: None,
                test: None,
                clean: None,
                build: None,
                flash: None,
                variables: None,
            },
            conflicts,
            notes,
        };
    }

    // --- Mirror ---
    let mirror = if let Some(m) = mirror_override {
        m.to_path_buf()
    } else {
        let first_mirror = &targets[0].1.mirror;
        for (name, cfg) in &targets[1..] {
            if cfg.mirror != *first_mirror {
                notes.push(format!(
                    "Target '{}' has different mirror '{}', using '{}' from '{}'",
                    name,
                    cfg.mirror.display(),
                    first_mirror.display(),
                    targets[0].0
                ));
            }
        }
        first_mirror.clone()
    };

    // --- Gits ---
    let mut git_owners: HashMap<String, Vec<String>> = HashMap::new();
    for (name, cfg) in targets {
        for git in &cfg.gits {
            git_owners
                .entry(git.name.clone())
                .or_default()
                .push(name.to_string());
        }
    }
    let mut merged_gits = Vec::new();
    for (name, cfg) in targets {
        for git in &cfg.gits {
            let owners = &git_owners[&git.name];
            if owners.len() > 1 {
                // Only report conflict once (from the first target that defines it)
                if owners[0] == *name {
                    conflicts.push(MergeConflict {
                        section: "gits".to_string(),
                        key: git.name.clone(),
                        sources: owners.clone(),
                    });
                }
            } else {
                merged_gits.push(git.clone());
            }
        }
    }

    // --- Toolchains ---
    let mut tc_owners: HashMap<String, Vec<String>> = HashMap::new();
    for (name, cfg) in targets {
        if let Some(ref tcs) = cfg.toolchains {
            for tc in tcs {
                tc_owners
                    .entry(tc.get_name())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }
    let mut merged_toolchains: Vec<ToolchainConfig> = Vec::new();
    for (name, cfg) in targets {
        if let Some(ref tcs) = cfg.toolchains {
            for tc in tcs {
                let tc_name = tc.get_name();
                let owners = &tc_owners[&tc_name];
                if owners.len() > 1 {
                    if owners[0] == *name {
                        conflicts.push(MergeConflict {
                            section: "toolchains".to_string(),
                            key: tc_name,
                            sources: owners.clone(),
                        });
                    }
                } else {
                    merged_toolchains.push(tc.clone());
                }
            }
        }
    }

    // --- Variables ---
    let mut var_values: HashMap<String, (String, String)> = HashMap::new(); // key -> (value, first_target)
    let mut merged_variables: HashMap<String, String> = HashMap::new();
    for (name, cfg) in targets {
        if let Some(ref vars) = cfg.variables {
            for (key, value) in vars {
                if let Some((existing_value, first_target)) = var_values.get(key) {
                    if value != existing_value {
                        conflicts.push(MergeConflict {
                            section: "variables".to_string(),
                            key: key.clone(),
                            sources: vec![first_target.clone(), name.to_string()],
                        });
                    }
                    // If values are identical, silently merge (no conflict)
                } else {
                    var_values.insert(key.clone(), (value.clone(), name.to_string()));
                    merged_variables.insert(key.clone(), value.clone());
                }
            }
        }
    }

    // --- Copy files ---
    let mut cf_owners: HashMap<String, Vec<String>> = HashMap::new(); // dest -> targets
    for (name, cfg) in targets {
        if let Some(ref cfs) = cfg.copy_files {
            for cf in cfs {
                cf_owners
                    .entry(cf.dest.clone())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }
    let mut merged_copy_files = Vec::new();
    for (name, cfg) in targets {
        if let Some(ref cfs) = cfg.copy_files {
            for cf in cfs {
                let owners = &cf_owners[&cf.dest];
                if owners.len() > 1 {
                    if owners[0] == *name {
                        conflicts.push(MergeConflict {
                            section: "copy_files".to_string(),
                            key: cf.dest.clone(),
                            sources: owners.clone(),
                        });
                    }
                } else {
                    merged_copy_files.push(cf.clone());
                }
            }
        }
    }

    // --- Install ---
    let mut install_owners: HashMap<String, Vec<String>> = HashMap::new();
    for (name, cfg) in targets {
        if let Some(ref installs) = cfg.install {
            for inst in installs {
                install_owners
                    .entry(inst.name.clone())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }
    let mut merged_install = Vec::new();
    for (name, cfg) in targets {
        if let Some(ref installs) = cfg.install {
            for inst in installs {
                let owners = &install_owners[&inst.name];
                if owners.len() > 1 {
                    if owners[0] == *name {
                        conflicts.push(MergeConflict {
                            section: "install".to_string(),
                            key: inst.name.clone(),
                            sources: owners.clone(),
                        });
                    }
                } else {
                    merged_install.push(inst.clone());
                }
            }
        }
    }

    // --- Makefile include (concatenate) ---
    let mut merged_mki_files: Vec<String> = Vec::new();
    let mut merged_mki_exclude: Vec<String> = Vec::new();
    for (_name, cfg) in targets {
        if let Some(ref mki) = cfg.makefile_include {
            merged_mki_files.extend(mki.files().iter().cloned());
            merged_mki_exclude.extend(mki.exclude().iter().cloned());
        }
    }
    // Deduplicate
    merged_mki_files.dedup();
    merged_mki_exclude.dedup();

    let merged_makefile_include = if merged_mki_files.is_empty() && merged_mki_exclude.is_empty() {
        None
    } else {
        Some(crate::config::MakefileInclude::Structured(
            crate::config::MakefileIncludeConfig {
                files: merged_mki_files,
                exclude: merged_mki_exclude,
            },
        ))
    };

    // --- Build folder (first non-None wins) ---
    let mut merged_build_folder = None;
    for (name, cfg) in targets {
        if cfg.build_folder.is_some() {
            if merged_build_folder.is_none() {
                merged_build_folder = cfg.build_folder.clone();
            } else if cfg.build_folder != merged_build_folder {
                notes.push(format!(
                    "Target '{}' has different build_folder '{}', using '{}' from earlier target",
                    name,
                    cfg.build_folder.as_deref().unwrap_or(""),
                    merged_build_folder.as_deref().unwrap_or("")
                ));
            }
        }
    }

    // --- Global targets: omitted ---
    // Collect which targets define each global target for informational notes
    let global_targets = ["build", "test", "clean", "flash", "envsetup"];
    for gt_name in &global_targets {
        let defining_targets: Vec<&str> = targets
            .iter()
            .filter(|(_name, cfg)| match *gt_name {
                "build" => cfg.build.is_some(),
                "test" => cfg.test.is_some(),
                "clean" => cfg.clean.is_some(),
                "flash" => cfg.flash.is_some(),
                "envsetup" => cfg.envsetup.is_some(),
                _ => false,
            })
            .map(|(name, _)| *name)
            .collect();
        if !defining_targets.is_empty() {
            notes.push(format!(
                "Global target '{}' defined in [{}] — omitted from merge, provide via fragment",
                gt_name,
                defining_targets.join(", ")
            ));
        }
    }

    let config = SdkConfig {
        mirror,
        gits: merged_gits,
        toolchains: if merged_toolchains.is_empty() {
            None
        } else {
            Some(merged_toolchains)
        },
        copy_files: if merged_copy_files.is_empty() {
            None
        } else {
            Some(merged_copy_files)
        },
        install: if merged_install.is_empty() {
            None
        } else {
            Some(merged_install)
        },
        makefile_include: merged_makefile_include,
        build_folder: merged_build_folder,
        envsetup: None,
        test: None,
        clean: None,
        build: None,
        flash: None,
        variables: if merged_variables.is_empty() {
            None
        } else {
            Some(merged_variables)
        },
    };

    MergeResult {
        config,
        conflicts,
        notes,
    }
}

/// Section divider used in formatted YAML output.
const DIVIDER: &str =
    "################################################################################";

/// Format a merged SdkConfig as clean YAML with section dividers and comments.
///
/// Produces output that matches the hand-written style of existing sdk.yml
/// files: section headers, blank lines between sections, and no `null` values.
pub fn format_merged_yaml(config: &SdkConfig, target_names: &[&str]) -> String {
    let mut out = String::new();

    // Header comment
    out.push_str(&format!("{}\n", DIVIDER));
    out.push_str(&format!(
        "# Merged SDK configuration ({})\n",
        target_names.join(" + ")
    ));
    out.push_str(&format!(
        "# Generated by: cim merge --targets {}\n",
        target_names.join(" ")
    ));
    out.push_str(&format!("{}\n", DIVIDER));

    // --- Variables ---
    if let Some(ref vars) = config.variables {
        if !vars.is_empty() {
            out.push('\n');
            emit_section_header(&mut out, "Manifest variables");
            out.push_str("variables:\n");
            // Sort for deterministic output
            let sorted: BTreeMap<_, _> = vars.iter().collect();
            for (key, value) in &sorted {
                out.push_str(&format!("  {}: {}\n", key, value));
            }
        }
    }

    // --- Mirror ---
    out.push('\n');
    emit_section_header(
        &mut out,
        "Mirror location for downloads, toolchains and gits",
    );
    out.push_str(&format!("mirror: {}\n", config.mirror.display()));

    // --- Makefile include ---
    if let Some(ref mki) = config.makefile_include {
        let files = mki.files();
        let exclude = mki.exclude();
        if !files.is_empty() || !exclude.is_empty() {
            out.push('\n');
            emit_section_header(&mut out, "Makefile includes");
            if exclude.is_empty() {
                // Legacy form
                out.push_str("makefile_include:\n");
                for f in files {
                    out.push_str(&format!("  - {}\n", f));
                }
            } else {
                // Structured form
                out.push_str("makefile_include:\n");
                if !files.is_empty() {
                    out.push_str("  files:\n");
                    for f in files {
                        out.push_str(&format!("    - {}\n", f));
                    }
                }
                out.push_str("  exclude:\n");
                for e in exclude {
                    out.push_str(&format!("    - {}\n", e));
                }
            }
        }
    }

    // --- Build folder ---
    if let Some(ref bf) = config.build_folder {
        out.push('\n');
        out.push_str(&format!("build_folder: {}\n", bf));
    }

    // --- Copy files ---
    if let Some(ref cfs) = config.copy_files {
        if !cfs.is_empty() {
            out.push('\n');
            emit_section_header(&mut out, "Files copied into the workspace");
            out.push_str("copy_files:\n");
            for cf in cfs {
                out.push_str(&format!("  - source: {}\n", cf.source));
                out.push_str(&format!("    dest: {}\n", cf.dest));
                if let Some(cache) = cf.cache {
                    out.push_str(&format!("    cache: {}\n", cache));
                }
                if let Some(ref sha) = cf.sha256 {
                    out.push_str(&format!("    sha256: {}\n", sha));
                }
                if let Some(ref post) = cf.post_data {
                    out.push_str(&format!("    post_data: {}\n", post));
                }
                if let Some(sym) = cf.symlink {
                    out.push_str(&format!("    symlink: {}\n", sym));
                }
            }
        }
    }

    // --- Toolchains ---
    if let Some(ref tcs) = config.toolchains {
        if !tcs.is_empty() {
            out.push('\n');
            emit_section_header(&mut out, "Toolchain configurations");
            out.push_str("toolchains:\n");
            for tc in tcs {
                emit_toolchain(&mut out, tc);
            }
        }
    }

    // --- Install ---
    if let Some(ref installs) = config.install {
        if !installs.is_empty() {
            out.push('\n');
            emit_section_header(&mut out, "Install steps");
            out.push_str("install:\n");
            for inst in installs {
                out.push_str(&format!("  - name: {}\n", inst.name));
                if let Some(ref deps) = inst.depends_on {
                    out.push_str("    depends_on:\n");
                    for d in deps {
                        out.push_str(&format!("      - {}\n", d));
                    }
                }
                if let Some(ref sentinel) = inst.sentinel {
                    out.push_str(&format!("    sentinel: {}\n", sentinel));
                }
                if let Some(ref cmds) = inst.commands {
                    emit_string_or_vec(&mut out, "commands", cmds, 4);
                }
            }
        }
    }

    // --- Global targets ---
    emit_optional_sdk_target(&mut out, "envsetup", &config.envsetup);
    emit_optional_sdk_target(&mut out, "build", &config.build);
    emit_optional_sdk_target(&mut out, "test", &config.test);
    emit_optional_sdk_target(&mut out, "clean", &config.clean);
    emit_optional_sdk_target(&mut out, "flash", &config.flash);

    // --- Gits ---
    if !config.gits.is_empty() {
        out.push('\n');
        emit_section_header(&mut out, "Git repositories");
        out.push_str("gits:\n");
        for git in &config.gits {
            out.push_str(&format!("  - name: {}\n", git.name));
            out.push_str(&format!("    url: {}\n", git.url));
            out.push_str(&format!("    commit: {}\n", git.commit));
            if let Some(ref deps) = git.build_depends_on {
                out.push_str("    build_depends_on:\n");
                for d in deps {
                    out.push_str(&format!("      - {}\n", d));
                }
            }
            if let Some(ref deps) = git.git_depends_on {
                out.push_str("    git_depends_on:\n");
                for d in deps {
                    out.push_str(&format!("      - {}\n", d));
                }
            }
            if let Some(ref cmds) = git.build {
                emit_string_or_vec(&mut out, "build", cmds, 4);
            }
            if let Some(ref doc_dir) = git.documentation_dir {
                out.push_str(&format!("    documentation_dir: {}\n", doc_dir));
            }
            out.push('\n');
        }
    }

    out
}

fn emit_section_header(out: &mut String, title: &str) {
    out.push_str(&format!("{}\n", DIVIDER));
    out.push_str(&format!("# {}\n", title));
    out.push_str(&format!("{}\n", DIVIDER));
}

fn emit_toolchain(out: &mut String, tc: &ToolchainConfig) {
    if let Some(ref name) = tc.name {
        out.push_str(&format!("  - name: {}\n", name));
    } else {
        out.push_str("  - url: ");
        // Handled below — skip url line here for named toolchains
    }
    if tc.name.is_some() {
        out.push_str(&format!("    url: {}\n", tc.url));
    } else {
        // Nameless toolchain: url is the first key
        out.push_str(&format!("{}\n", tc.url));
    }
    out.push_str(&format!("    destination: {}\n", tc.destination));
    if let Some(sc) = tc.strip_components {
        out.push_str(&format!("    strip_components: {}\n", sc));
    }
    if let Some(ref os) = tc.os {
        out.push_str(&format!("    os: {}\n", os));
    }
    if let Some(ref arch) = tc.arch {
        out.push_str(&format!("    arch: {}\n", arch));
    }
    if let Some(ref sha) = tc.sha256 {
        out.push_str(&format!("    sha256: {}\n", sha));
    }
    if let Some(ref md) = tc.mirror_destination {
        out.push_str(&format!("    mirror_destination: {}\n", md));
    }
    if let Some(ref env) = tc.environment {
        out.push_str("    environment:\n");
        for (k, v) in env {
            out.push_str(&format!("      {}: \"{}\"\n", k, v));
        }
    }
    if let Some(ref cmds) = tc.post_install_commands {
        emit_string_or_vec(out, "post_install_commands", cmds, 4);
    }
    out.push('\n');
}

fn emit_string_or_vec(out: &mut String, key: &str, values: &[String], indent: usize) {
    let prefix = " ".repeat(indent);
    if values.len() == 1 && values[0].contains('\n') {
        // Multi-line block scalar
        out.push_str(&format!("{}{}: |\n", prefix, key));
        for line in values[0].lines() {
            out.push_str(&format!("{}  {}\n", prefix, line));
        }
    } else {
        out.push_str(&format!("{}{}:\n", prefix, key));
        for v in values {
            out.push_str(&format!("{}  - {}\n", prefix, v));
        }
    }
}

fn emit_optional_sdk_target(
    out: &mut String,
    name: &str,
    target: &Option<crate::config::SdkTarget>,
) {
    use crate::config::SdkTarget;
    let Some(t) = target else { return };

    out.push('\n');
    match t {
        SdkTarget::Commands(cmds) => {
            emit_string_or_vec(out, name, cmds, 0);
        }
        SdkTarget::CommandsWithDeps {
            commands,
            depends_on,
        } => {
            out.push_str(&format!("{}:\n", name));
            if let Some(ref deps) = depends_on {
                out.push_str("  depends_on:\n");
                for d in deps {
                    out.push_str(&format!("    - {}\n", d));
                }
            }
            emit_string_or_vec(out, "commands", commands, 2);
        }
    }
}

/// Merge multiple os-dependencies.yml files.
///
/// For each OS/distro combination, packages are combined into a sorted,
/// deduplicated union. The install command from the first target defining
/// that distro wins.
pub fn merge_os_dependencies(deps_list: &[(&str, &OsDependencies)]) -> OsDependencies {
    let mut merged: HashMap<String, OsConfig> = HashMap::new();

    for (_target_name, deps) in deps_list {
        for (os_key, os_config) in &deps.os_configs {
            let entry = merged.entry(os_key.clone()).or_insert_with(|| OsConfig {
                distros: HashMap::new(),
            });

            for (distro_key, distro_config) in &os_config.distros {
                let distro_entry = entry.distros.entry(distro_key.clone()).or_insert_with(|| {
                    crate::config::DistroConfig {
                        version: distro_config.version.clone(),
                        package_manager: crate::config::PackageManagerConfig {
                            command: distro_config.package_manager.command.clone(),
                            packages: Vec::new(),
                        },
                    }
                });

                // Union of packages (deduplicated, sorted at the end)
                for pkg in &distro_config.package_manager.packages {
                    if !distro_entry.package_manager.packages.contains(pkg) {
                        distro_entry.package_manager.packages.push(pkg.clone());
                    }
                }
            }
        }
    }

    // Sort packages within each distro for deterministic output
    for os_config in merged.values_mut() {
        for distro_config in os_config.distros.values_mut() {
            distro_config.package_manager.packages.sort();
        }
    }

    OsDependencies { os_configs: merged }
}

/// Format merged os-dependencies.yml as clean YAML.
///
/// Groups by OS key (linux-x86_64, linux-aarch64, macos, etc.) with
/// section headers and sorted packages.
pub fn format_os_dependencies_yaml(deps: &OsDependencies) -> String {
    let mut out = String::new();

    out.push_str(&format!("{}\n", DIVIDER));
    out.push_str("# Merged OS dependencies\n");
    out.push_str(&format!("{}\n", DIVIDER));

    // Sort OS keys for deterministic output
    let sorted_os: BTreeMap<_, _> = deps.os_configs.iter().collect();

    for (os_key, os_config) in &sorted_os {
        out.push('\n');
        out.push_str(&format!("{}:\n", os_key));

        let sorted_distros: BTreeMap<_, _> = os_config.distros.iter().collect();
        for (distro_key, distro_config) in &sorted_distros {
            out.push_str(&format!("  {}:\n", distro_key));
            out.push_str(&format!(
                "    command: \"{}\"\n",
                distro_config.package_manager.command
            ));
            out.push_str("    packages:\n");
            for pkg in &distro_config.package_manager.packages {
                out.push_str(&format!("      - {}\n", pkg));
            }
        }
    }

    out
}

/// Merge multiple python-dependencies.yml files.
///
/// For each profile, packages are combined into a sorted, deduplicated
/// union. The default profile is taken from the first target.
pub fn merge_python_dependencies(deps_list: &[(&str, &PythonDependencies)]) -> PythonDependencies {
    let mut merged_profiles: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut default_profile = String::from("docs");

    for (i, (_target_name, deps)) in deps_list.iter().enumerate() {
        if i == 0 {
            default_profile = deps.default.clone();
        }

        for (profile_name, profile) in &deps.profiles {
            let entry = merged_profiles.entry(profile_name.clone()).or_default();
            for pkg in &profile.packages {
                entry.insert(pkg.clone());
            }
        }
    }

    let profiles = merged_profiles
        .into_iter()
        .map(|(name, pkgs)| {
            (
                name,
                PythonProfile {
                    packages: pkgs.into_iter().collect(),
                },
            )
        })
        .collect();

    PythonDependencies {
        profiles,
        default: default_profile,
    }
}

/// Format merged python-dependencies.yml as clean YAML.
pub fn format_python_dependencies_yaml(deps: &PythonDependencies) -> String {
    let mut out = String::new();

    out.push_str(&format!("{}\n", DIVIDER));
    out.push_str("# Merged Python dependencies\n");
    out.push_str(&format!("{}\n", DIVIDER));

    out.push_str("\nprofiles:\n");

    // Sort profiles for deterministic output
    let sorted: BTreeMap<_, _> = deps.profiles.iter().collect();
    for (name, profile) in &sorted {
        out.push_str(&format!("  {}:\n", name));
        out.push_str("    packages:\n");
        if profile.packages.is_empty() {
            out.push_str("      []\n");
        } else {
            for pkg in &profile.packages {
                out.push_str(&format!("      - {}\n", pkg));
            }
        }
        out.push('\n');
    }

    out.push_str(&format!("default: {}\n", deps.default));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CopyFileConfig, GitConfig, InstallConfig, MakefileInclude, MakefileIncludeConfig,
        SdkConfig, SdkTarget, ToolchainConfig,
    };
    use std::path::PathBuf;

    fn make_config(name: &str) -> SdkConfig {
        SdkConfig {
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![GitConfig {
                name: name.to_string(),
                url: format!("https://example.com/{}.git", name),
                commit: "main".to_string(),
                build_depends_on: None,
                git_depends_on: None,
                build: Some(vec![format!("make -C {}", name)]),
                documentation_dir: None,
            }],
            toolchains: None,
            copy_files: None,
            install: None,
            makefile_include: None,
            build_folder: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        }
    }

    #[test]
    fn test_merge_disjoint_gits() {
        let cfg_a = make_config("repo-a");
        let cfg_b = make_config("repo-b");

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        assert_eq!(result.config.gits.len(), 2);
        assert!(result.config.gits.iter().any(|g| g.name == "repo-a"));
        assert!(result.config.gits.iter().any(|g| g.name == "repo-b"));
    }

    #[test]
    fn test_merge_conflicting_gits() {
        let cfg_a = make_config("shared-repo");
        let cfg_b = make_config("shared-repo");

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].section, "gits");
        assert_eq!(result.conflicts[0].key, "shared-repo");
        assert_eq!(result.conflicts[0].sources, vec!["target-a", "target-b"]);
        // Conflicting gits are excluded from the merged config
        assert!(result.config.gits.is_empty());
    }

    #[test]
    fn test_merge_disjoint_toolchains() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.toolchains = Some(vec![ToolchainConfig {
            name: Some("gcc-arm".to_string()),
            url: "https://example.com/gcc-arm.tar.xz".to_string(),
            destination: "/opt/gcc-arm".to_string(),
            strip_components: None,
            os: None,
            arch: None,
            sha256: None,
            mirror_destination: None,
            environment: None,
            post_install_commands: None,
        }]);

        let mut cfg_b = make_config("repo-b");
        cfg_b.toolchains = Some(vec![ToolchainConfig {
            name: Some("gcc-riscv".to_string()),
            url: "https://example.com/gcc-riscv.tar.xz".to_string(),
            destination: "/opt/gcc-riscv".to_string(),
            strip_components: None,
            os: None,
            arch: None,
            sha256: None,
            mirror_destination: None,
            environment: None,
            post_install_commands: None,
        }]);

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        let tcs = result.config.toolchains.as_ref().unwrap();
        assert_eq!(tcs.len(), 2);
    }

    #[test]
    fn test_merge_conflicting_toolchains() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.toolchains = Some(vec![ToolchainConfig {
            name: Some("gcc-arm".to_string()),
            url: "https://example.com/gcc-arm-v12.tar.xz".to_string(),
            destination: "/opt/gcc-arm".to_string(),
            strip_components: None,
            os: None,
            arch: None,
            sha256: None,
            mirror_destination: None,
            environment: None,
            post_install_commands: None,
        }]);

        let mut cfg_b = make_config("repo-b");
        cfg_b.toolchains = Some(vec![ToolchainConfig {
            name: Some("gcc-arm".to_string()),
            url: "https://example.com/gcc-arm-v13.tar.xz".to_string(),
            destination: "/opt/gcc-arm".to_string(),
            strip_components: None,
            os: None,
            arch: None,
            sha256: None,
            mirror_destination: None,
            environment: None,
            post_install_commands: None,
        }]);

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].section, "toolchains");
        assert_eq!(result.conflicts[0].key, "gcc-arm");
    }

    #[test]
    fn test_merge_variables_disjoint() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.variables = Some(HashMap::from([(
            "VAR_A".to_string(),
            "value_a".to_string(),
        )]));

        let mut cfg_b = make_config("repo-b");
        cfg_b.variables = Some(HashMap::from([(
            "VAR_B".to_string(),
            "value_b".to_string(),
        )]));

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        let vars = result.config.variables.as_ref().unwrap();
        assert_eq!(vars["VAR_A"], "value_a");
        assert_eq!(vars["VAR_B"], "value_b");
    }

    #[test]
    fn test_merge_variables_identical_no_conflict() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.variables = Some(HashMap::from([(
            "SHARED".to_string(),
            "same_value".to_string(),
        )]));

        let mut cfg_b = make_config("repo-b");
        cfg_b.variables = Some(HashMap::from([(
            "SHARED".to_string(),
            "same_value".to_string(),
        )]));

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        let vars = result.config.variables.as_ref().unwrap();
        assert_eq!(vars["SHARED"], "same_value");
    }

    #[test]
    fn test_merge_variables_conflict_different_values() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.variables = Some(HashMap::from([(
            "SHARED".to_string(),
            "value_a".to_string(),
        )]));

        let mut cfg_b = make_config("repo-b");
        cfg_b.variables = Some(HashMap::from([(
            "SHARED".to_string(),
            "value_b".to_string(),
        )]));

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].section, "variables");
        assert_eq!(result.conflicts[0].key, "SHARED");
    }

    #[test]
    fn test_merge_copy_files_disjoint() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.copy_files = Some(vec![CopyFileConfig {
            source: "a/file.sh".to_string(),
            dest: "scripts/a.sh".to_string(),
            cache: None,
            sha256: None,
            post_data: None,
            symlink: None,
        }]);

        let mut cfg_b = make_config("repo-b");
        cfg_b.copy_files = Some(vec![CopyFileConfig {
            source: "b/file.sh".to_string(),
            dest: "scripts/b.sh".to_string(),
            cache: None,
            sha256: None,
            post_data: None,
            symlink: None,
        }]);

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        assert_eq!(result.config.copy_files.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_merge_copy_files_conflict() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.copy_files = Some(vec![CopyFileConfig {
            source: "a/setup.sh".to_string(),
            dest: "scripts/setup.sh".to_string(),
            cache: None,
            sha256: None,
            post_data: None,
            symlink: None,
        }]);

        let mut cfg_b = make_config("repo-b");
        cfg_b.copy_files = Some(vec![CopyFileConfig {
            source: "b/setup.sh".to_string(),
            dest: "scripts/setup.sh".to_string(),
            cache: None,
            sha256: None,
            post_data: None,
            symlink: None,
        }]);

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].section, "copy_files");
        assert_eq!(result.conflicts[0].key, "scripts/setup.sh");
    }

    #[test]
    fn test_merge_install_disjoint() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.install = Some(vec![InstallConfig {
            name: "tool-a".to_string(),
            depends_on: None,
            sentinel: None,
            commands: Some(vec!["install-a".to_string()]),
        }]);

        let mut cfg_b = make_config("repo-b");
        cfg_b.install = Some(vec![InstallConfig {
            name: "tool-b".to_string(),
            depends_on: None,
            sentinel: None,
            commands: Some(vec!["install-b".to_string()]),
        }]);

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        assert_eq!(result.config.install.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_merge_install_conflict() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.install = Some(vec![InstallConfig {
            name: "ninja".to_string(),
            depends_on: None,
            sentinel: None,
            commands: Some(vec!["apt install ninja".to_string()]),
        }]);

        let mut cfg_b = make_config("repo-b");
        cfg_b.install = Some(vec![InstallConfig {
            name: "ninja".to_string(),
            depends_on: None,
            sentinel: None,
            commands: Some(vec!["pip install ninja".to_string()]),
        }]);

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].section, "install");
    }

    #[test]
    fn test_merge_mirror_override() {
        let cfg_a = make_config("repo-a");
        let cfg_b = make_config("repo-b");
        let override_path = PathBuf::from("/custom/mirror");

        let result = merge_targets(
            &[("target-a", &cfg_a), ("target-b", &cfg_b)],
            Some(&override_path),
        );

        assert_eq!(result.config.mirror, override_path);
    }

    #[test]
    fn test_merge_global_targets_omitted_with_notes() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.build = Some(SdkTarget::Commands(vec!["make all-a".to_string()]));
        cfg_a.test = Some(SdkTarget::Commands(vec!["make test-a".to_string()]));

        let mut cfg_b = make_config("repo-b");
        cfg_b.build = Some(SdkTarget::Commands(vec!["make all-b".to_string()]));

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        // Global targets should be omitted
        assert!(result.config.build.is_none());
        assert!(result.config.test.is_none());
        // Notes should mention which targets defined them
        assert!(result.notes.iter().any(|n| n.contains("build")));
        assert!(result.notes.iter().any(|n| n.contains("test")));
    }

    #[test]
    fn test_merge_makefile_include_concatenated() {
        let mut cfg_a = make_config("repo-a");
        cfg_a.makefile_include = Some(MakefileInclude::Structured(MakefileIncludeConfig {
            files: vec!["include a.mk".to_string()],
            exclude: vec!["excluded-a".to_string()],
        }));

        let mut cfg_b = make_config("repo-b");
        cfg_b.makefile_include = Some(MakefileInclude::Structured(MakefileIncludeConfig {
            files: vec!["include b.mk".to_string()],
            exclude: vec!["excluded-b".to_string()],
        }));

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        assert!(result.conflicts.is_empty());
        let mki = result.config.makefile_include.as_ref().unwrap();
        assert_eq!(mki.files().len(), 2);
        assert_eq!(mki.exclude().len(), 2);
    }

    #[test]
    fn test_merge_three_targets() {
        let cfg_a = make_config("repo-a");
        let cfg_b = make_config("repo-b");
        let cfg_c = make_config("repo-c");

        let result = merge_targets(
            &[
                ("target-a", &cfg_a),
                ("target-b", &cfg_b),
                ("target-c", &cfg_c),
            ],
            None,
        );

        assert!(result.conflicts.is_empty());
        assert_eq!(result.config.gits.len(), 3);
    }

    #[test]
    fn test_merge_empty_targets() {
        let result = merge_targets(&[], None);

        assert!(result.conflicts.is_empty());
        assert!(result.config.gits.is_empty());
    }

    #[test]
    fn test_merge_mixed_conflicts_and_successes() {
        let mut cfg_a = make_config("shared-repo");
        cfg_a.gits.push(GitConfig {
            name: "unique-a".to_string(),
            url: "https://example.com/unique-a.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: None,
            documentation_dir: None,
        });
        cfg_a.variables = Some(HashMap::from([
            ("SHARED_VAR".to_string(), "different_a".to_string()),
            ("UNIQUE_A".to_string(), "a_val".to_string()),
        ]));

        let mut cfg_b = make_config("shared-repo");
        cfg_b.gits.push(GitConfig {
            name: "unique-b".to_string(),
            url: "https://example.com/unique-b.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: None,
            documentation_dir: None,
        });
        cfg_b.variables = Some(HashMap::from([
            ("SHARED_VAR".to_string(), "different_b".to_string()),
            ("UNIQUE_B".to_string(), "b_val".to_string()),
        ]));

        let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

        // Should have 2 conflicts: shared-repo git + SHARED_VAR variable
        assert_eq!(result.conflicts.len(), 2);

        // Non-conflicting items should still be merged
        assert_eq!(result.config.gits.len(), 2); // unique-a + unique-b
        assert!(result.config.gits.iter().any(|g| g.name == "unique-a"));
        assert!(result.config.gits.iter().any(|g| g.name == "unique-b"));

        let vars = result.config.variables.as_ref().unwrap();
        assert_eq!(vars["UNIQUE_A"], "a_val");
        assert_eq!(vars["UNIQUE_B"], "b_val");
    }

    #[test]
    fn test_format_merged_yaml_no_nulls() {
        let cfg = make_config("repo-a");
        let yaml = format_merged_yaml(&cfg, &["target-a"]);

        assert!(
            !yaml.contains("null"),
            "YAML should not contain null values"
        );
        assert!(yaml.contains("# Merged SDK configuration"));
        assert!(yaml.contains("# Git repositories"));
        assert!(yaml.contains("repo-a"));
    }

    #[test]
    fn test_format_merged_yaml_section_dividers() {
        let mut cfg = make_config("repo-a");
        cfg.toolchains = Some(vec![ToolchainConfig {
            name: Some("gcc-arm".to_string()),
            url: "https://example.com/gcc-arm.tar.xz".to_string(),
            destination: "/opt/gcc-arm".to_string(),
            strip_components: None,
            os: None,
            arch: None,
            sha256: None,
            mirror_destination: None,
            environment: None,
            post_install_commands: None,
        }]);
        cfg.variables = Some(HashMap::from([("VAR_A".to_string(), "val_a".to_string())]));

        let yaml = format_merged_yaml(&cfg, &["target-a", "target-b"]);

        assert!(yaml.contains("# Toolchain configurations"));
        assert!(yaml.contains("# Manifest variables"));
        assert!(yaml.contains("# Mirror location"));
        // Verify section order
        let mirror_pos = yaml.find("mirror:").unwrap();
        let tc_pos = yaml.find("toolchains:").unwrap();
        let gits_pos = yaml.find("gits:").unwrap();
        assert!(mirror_pos < tc_pos);
        assert!(tc_pos < gits_pos);
    }

    #[test]
    fn test_merge_os_dependencies_union() {
        use crate::config::{DistroConfig, OsConfig, PackageManagerConfig};

        let deps_a = OsDependencies {
            os_configs: HashMap::from([(
                "linux-x86_64".to_string(),
                OsConfig {
                    distros: HashMap::from([(
                        "ubuntu-24.04".to_string(),
                        DistroConfig {
                            version: None,
                            package_manager: PackageManagerConfig {
                                command: "apt install".to_string(),
                                packages: vec!["git".to_string(), "curl".to_string()],
                            },
                        },
                    )]),
                },
            )]),
        };

        let deps_b = OsDependencies {
            os_configs: HashMap::from([(
                "linux-x86_64".to_string(),
                OsConfig {
                    distros: HashMap::from([(
                        "ubuntu-24.04".to_string(),
                        DistroConfig {
                            version: None,
                            package_manager: PackageManagerConfig {
                                command: "apt install".to_string(),
                                packages: vec![
                                    "git".to_string(),
                                    "cmake".to_string(),
                                    "bison".to_string(),
                                ],
                            },
                        },
                    )]),
                },
            )]),
        };

        let merged = merge_os_dependencies(&[("target-a", &deps_a), ("target-b", &deps_b)]);

        let pkgs = &merged.os_configs["linux-x86_64"].distros["ubuntu-24.04"]
            .package_manager
            .packages;
        assert_eq!(pkgs, &["bison", "cmake", "curl", "git"]); // sorted, deduplicated
    }

    #[test]
    fn test_merge_os_dependencies_disjoint_os() {
        use crate::config::{DistroConfig, OsConfig, PackageManagerConfig};

        let deps_a = OsDependencies {
            os_configs: HashMap::from([(
                "linux-x86_64".to_string(),
                OsConfig {
                    distros: HashMap::from([(
                        "ubuntu-24.04".to_string(),
                        DistroConfig {
                            version: None,
                            package_manager: PackageManagerConfig {
                                command: "apt install".to_string(),
                                packages: vec!["git".to_string()],
                            },
                        },
                    )]),
                },
            )]),
        };

        let deps_b = OsDependencies {
            os_configs: HashMap::from([(
                "macos".to_string(),
                OsConfig {
                    distros: HashMap::from([(
                        "macos-any".to_string(),
                        DistroConfig {
                            version: None,
                            package_manager: PackageManagerConfig {
                                command: "brew install".to_string(),
                                packages: vec!["git".to_string()],
                            },
                        },
                    )]),
                },
            )]),
        };

        let merged = merge_os_dependencies(&[("target-a", &deps_a), ("target-b", &deps_b)]);

        assert!(merged.os_configs.contains_key("linux-x86_64"));
        assert!(merged.os_configs.contains_key("macos"));
    }

    #[test]
    fn test_merge_python_dependencies_union() {
        let deps_a = PythonDependencies {
            profiles: HashMap::from([(
                "docs".to_string(),
                PythonProfile {
                    packages: vec!["sphinx".to_string(), "myst-parser".to_string()],
                },
            )]),
            default: "docs".to_string(),
        };

        let deps_b = PythonDependencies {
            profiles: HashMap::from([
                (
                    "docs".to_string(),
                    PythonProfile {
                        packages: vec!["sphinx".to_string(), "sphinx-rtd-theme".to_string()],
                    },
                ),
                (
                    "dev".to_string(),
                    PythonProfile {
                        packages: vec!["pytest".to_string()],
                    },
                ),
            ]),
            default: "dev".to_string(),
        };

        let merged = merge_python_dependencies(&[("target-a", &deps_a), ("target-b", &deps_b)]);

        // docs profile should be a union
        let docs_pkgs = &merged.profiles["docs"].packages;
        assert!(docs_pkgs.contains(&"sphinx".to_string()));
        assert!(docs_pkgs.contains(&"myst-parser".to_string()));
        assert!(docs_pkgs.contains(&"sphinx-rtd-theme".to_string()));

        // dev profile from target-b should be present
        assert!(merged.profiles.contains_key("dev"));

        // Default from first target
        assert_eq!(merged.default, "docs");
    }

    #[test]
    fn test_format_os_dependencies_yaml_sorted() {
        use crate::config::{DistroConfig, OsConfig, PackageManagerConfig};

        let deps = OsDependencies {
            os_configs: HashMap::from([
                (
                    "macos".to_string(),
                    OsConfig {
                        distros: HashMap::from([(
                            "macos-any".to_string(),
                            DistroConfig {
                                version: None,
                                package_manager: PackageManagerConfig {
                                    command: "brew install".to_string(),
                                    packages: vec!["git".to_string()],
                                },
                            },
                        )]),
                    },
                ),
                (
                    "linux-x86_64".to_string(),
                    OsConfig {
                        distros: HashMap::from([(
                            "ubuntu-24.04".to_string(),
                            DistroConfig {
                                version: None,
                                package_manager: PackageManagerConfig {
                                    command: "apt install".to_string(),
                                    packages: vec!["cmake".to_string(), "git".to_string()],
                                },
                            },
                        )]),
                    },
                ),
            ]),
        };

        let yaml = format_os_dependencies_yaml(&deps);
        // linux-x86_64 should come before macos (sorted)
        let linux_pos = yaml.find("linux-x86_64:").unwrap();
        let macos_pos = yaml.find("macos:").unwrap();
        assert!(linux_pos < macos_pos);
        assert!(yaml.contains("# Merged OS dependencies"));
    }

    #[test]
    fn test_format_python_dependencies_yaml() {
        let deps = PythonDependencies {
            profiles: HashMap::from([(
                "docs".to_string(),
                PythonProfile {
                    packages: vec!["sphinx".to_string()],
                },
            )]),
            default: "docs".to_string(),
        };

        let yaml = format_python_dependencies_yaml(&deps);
        assert!(yaml.contains("# Merged Python dependencies"));
        assert!(yaml.contains("profiles:"));
        assert!(yaml.contains("docs:"));
        assert!(yaml.contains("default: docs"));
    }
}
