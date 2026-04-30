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

use std::collections::HashMap;

use crate::config::{SdkConfig, ToolchainConfig};

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
}
