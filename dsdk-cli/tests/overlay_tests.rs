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

//! Integration tests for the extends:/overlay.yml merge engine (dsdk_cli::overlay).

mod common;

use common::{create_complex_sdk_config, create_minimal_sdk_config};
use dsdk_cli::config::{CopyFileConfig, GitConfig, InstallConfig, ToolchainConfig};
use dsdk_cli::overlay::{
    apply_overlay, compute_owned_entries, merge_copy_files, merge_gits, merge_install,
    merge_toolchains, merge_variables, validate_dependencies, CopyFilePatch, CopyFilesOverlay,
    GitPatch, GitsOverlay, InstallOverlay, InstallPatch, OverlayConfig, ToolchainPatch,
    ToolchainsOverlay, VariablesOverlay,
};
use std::collections::HashMap;

fn new_git(name: &str, url: &str, commit: &str) -> GitConfig {
    GitConfig {
        name: name.to_string(),
        url: url.to_string(),
        commit: commit.to_string(),
        build_depends_on: None,
        git_depends_on: None,
        build: None,
        documentation_dir: None,
        python_deps: None,
        group: None,
    }
}

fn new_toolchain(name: &str) -> ToolchainConfig {
    ToolchainConfig {
        name: Some(name.to_string()),
        url: "https://example.com/downloads".to_string(),
        destination: "toolchains/test".to_string(),
        strip_components: None,
        os: None,
        arch: None,
        sha256: None,
        mirror_destination: None,
        environment: None,
        post_install_commands: None,
    }
}

fn new_install(name: &str, depends_on: Option<Vec<&str>>) -> InstallConfig {
    InstallConfig {
        name: name.to_string(),
        depends_on: depends_on.map(|v| v.into_iter().map(String::from).collect()),
        sentinel: None,
        commands: None,
    }
}

fn new_copy_file(dest: &str) -> CopyFileConfig {
    CopyFileConfig {
        source: format!("https://example.com/{}", dest),
        dest: dest.to_string(),
        cache: None,
        sha256: None,
        post_data: None,
        symlink: None,
    }
}

// ---------------------------------------------------------------------
// gits: merge tests
// ---------------------------------------------------------------------

#[test]
fn test_merge_gits_add() {
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![new_git(
            "drone-camera",
            "https://example.com/camera.git",
            "main",
        )],
        remove: vec![],
        modify: vec![],
    };

    let merged = merge_gits(base, Some(&overlay)).expect("merge should succeed");
    assert_eq!(merged.len(), 4);
    assert!(merged.iter().any(|g| g.name == "drone-camera"));
}

#[test]
fn test_merge_gits_remove() {
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![],
        remove: vec!["application".to_string()],
        modify: vec![],
    };

    let merged = merge_gits(base, Some(&overlay)).expect("merge should succeed");
    assert_eq!(merged.len(), 2);
    assert!(!merged.iter().any(|g| g.name == "application"));
}

#[test]
fn test_merge_gits_modify_overrides_only_patched_fields() {
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![],
        remove: vec![],
        modify: vec![GitPatch {
            name: "middleware".to_string(),
            url: None,
            commit: Some("v2.0.0".to_string()),
            build_depends_on: None,
            git_depends_on: None,
            build: None,
            documentation_dir: None,
            python_deps: None,
            group: None,
        }],
    };

    let merged = merge_gits(base, Some(&overlay)).expect("merge should succeed");
    let middleware = merged.iter().find(|g| g.name == "middleware").unwrap();
    assert_eq!(middleware.commit, "v2.0.0");
    // Unpatched fields preserved
    assert_eq!(middleware.url, "https://github.com/example/middleware.git");
    assert_eq!(
        middleware.build_depends_on,
        Some(vec!["base-lib".to_string()])
    );
}

#[test]
fn test_merge_gits_remove_missing_errors() {
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![],
        remove: vec!["does-not-exist".to_string()],
        modify: vec![],
    };

    let err = merge_gits(base, Some(&overlay)).unwrap_err();
    assert!(err.contains("does-not-exist"));
    assert!(err.contains("not found in base"));
}

#[test]
fn test_merge_gits_modify_missing_errors() {
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![],
        remove: vec![],
        modify: vec![GitPatch {
            name: "does-not-exist".to_string(),
            url: None,
            commit: Some("v2.0.0".to_string()),
            build_depends_on: None,
            git_depends_on: None,
            build: None,
            documentation_dir: None,
            python_deps: None,
            group: None,
        }],
    };

    let err = merge_gits(base, Some(&overlay)).unwrap_err();
    assert!(err.contains("does-not-exist"));
}

#[test]
fn test_merge_gits_add_duplicate_errors() {
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![new_git("middleware", "https://example.com/dup.git", "main")],
        remove: vec![],
        modify: vec![],
    };

    let err = merge_gits(base, Some(&overlay)).unwrap_err();
    assert!(err.contains("middleware"));
    assert!(err.contains("already exists"));
}

#[test]
fn test_merge_gits_order_remove_then_add_allows_replacement() {
    // Remove then add a git with the same name but different URL should
    // succeed, proving remove is applied before the add-duplicate check.
    let base = create_complex_sdk_config().gits;
    let overlay = GitsOverlay {
        add: vec![new_git(
            "middleware",
            "https://example.com/replaced.git",
            "main",
        )],
        remove: vec!["middleware".to_string()],
        modify: vec![],
    };

    let merged = merge_gits(base, Some(&overlay)).expect("merge should succeed");
    let middleware = merged.iter().find(|g| g.name == "middleware").unwrap();
    assert_eq!(middleware.url, "https://example.com/replaced.git");
}

#[test]
fn test_merge_gits_none_overlay_is_noop() {
    let base = create_complex_sdk_config().gits;
    let base_len = base.len();
    let merged = merge_gits(base, None).expect("merge should succeed");
    assert_eq!(merged.len(), base_len);
}

// ---------------------------------------------------------------------
// toolchains: merge tests
// ---------------------------------------------------------------------

#[test]
fn test_merge_toolchains_add_remove_modify() {
    let base = Some(vec![new_toolchain("a"), new_toolchain("b")]);
    let overlay = ToolchainsOverlay {
        add: vec![new_toolchain("c")],
        remove: vec!["a".to_string()],
        modify: vec![ToolchainPatch {
            name: "b".to_string(),
            url: None,
            destination: Some("toolchains/patched".to_string()),
            strip_components: None,
            os: None,
            arch: None,
            sha256: None,
            mirror_destination: None,
            environment: None,
            post_install_commands: None,
        }],
    };

    let merged = merge_toolchains(base, Some(&overlay))
        .expect("merge should succeed")
        .unwrap();
    assert_eq!(merged.len(), 2);
    assert!(!merged.iter().any(|t| t.get_name() == "a"));
    assert!(merged.iter().any(|t| t.get_name() == "c"));
    let patched = merged.iter().find(|t| t.get_name() == "b").unwrap();
    assert_eq!(patched.destination, "toolchains/patched");
}

// ---------------------------------------------------------------------
// install: merge tests
// ---------------------------------------------------------------------

#[test]
fn test_merge_install_add_remove_modify() {
    let base = Some(vec![new_install("a", None), new_install("b", None)]);
    let overlay = InstallOverlay {
        add: vec![new_install("c", Some(vec!["b"]))],
        remove: vec!["a".to_string()],
        modify: vec![InstallPatch {
            name: "b".to_string(),
            depends_on: None,
            sentinel: Some(".cim/.b-installed".to_string()),
            commands: None,
        }],
    };

    let merged = merge_install(base, Some(&overlay))
        .expect("merge should succeed")
        .unwrap();
    assert_eq!(merged.len(), 2);
    let b = merged.iter().find(|i| i.name == "b").unwrap();
    assert_eq!(b.sentinel.as_deref(), Some(".cim/.b-installed"));
    let c = merged.iter().find(|i| i.name == "c").unwrap();
    assert_eq!(c.depends_on, Some(vec!["b".to_string()]));
}

// ---------------------------------------------------------------------
// copy_files: merge tests (keyed by dest)
// ---------------------------------------------------------------------

#[test]
fn test_merge_copy_files_add_remove_modify() {
    let base = Some(vec![
        new_copy_file("patches/a.patch"),
        new_copy_file("patches/b.patch"),
    ]);
    let overlay = CopyFilesOverlay {
        add: vec![new_copy_file("patches/c.patch")],
        remove: vec!["patches/a.patch".to_string()],
        modify: vec![CopyFilePatch {
            dest: "patches/b.patch".to_string(),
            source: Some("https://example.com/new-b".to_string()),
            cache: Some(true),
            sha256: None,
            post_data: None,
            symlink: None,
        }],
    };

    let merged = merge_copy_files(base, Some(&overlay))
        .expect("merge should succeed")
        .unwrap();
    assert_eq!(merged.len(), 2);
    assert!(!merged.iter().any(|c| c.dest == "patches/a.patch"));
    let b = merged.iter().find(|c| c.dest == "patches/b.patch").unwrap();
    assert_eq!(b.source, "https://example.com/new-b");
    assert_eq!(b.cache, Some(true));
}

#[test]
fn test_merge_copy_files_remove_missing_errors() {
    let base = Some(vec![new_copy_file("patches/a.patch")]);
    let overlay = CopyFilesOverlay {
        add: vec![],
        remove: vec!["patches/missing.patch".to_string()],
        modify: vec![],
    };

    let err = merge_copy_files(base, Some(&overlay)).unwrap_err();
    assert!(err.contains("patches/missing.patch"));
}

// ---------------------------------------------------------------------
// variables: merge tests
// ---------------------------------------------------------------------

#[test]
fn test_merge_variables_set_and_remove() {
    let mut base = HashMap::new();
    base.insert("ZEPHYR_BOARD".to_string(), "board-a".to_string());
    base.insert("KEEP_ME".to_string(), "value".to_string());

    let mut set = HashMap::new();
    set.insert("ZEPHYR_BOARD".to_string(), "board-b".to_string());
    set.insert("NEW_VAR".to_string(), "new-value".to_string());

    let overlay = VariablesOverlay {
        set,
        remove: vec!["KEEP_ME".to_string()],
    };

    let merged = merge_variables(Some(base), Some(&overlay))
        .expect("merge should succeed")
        .unwrap();
    assert_eq!(merged.get("ZEPHYR_BOARD"), Some(&"board-b".to_string()));
    assert_eq!(merged.get("NEW_VAR"), Some(&"new-value".to_string()));
    assert!(!merged.contains_key("KEEP_ME"));
}

#[test]
fn test_merge_variables_remove_missing_errors() {
    let overlay = VariablesOverlay {
        set: HashMap::new(),
        remove: vec!["DOES_NOT_EXIST".to_string()],
    };

    let err = merge_variables(None, Some(&overlay)).unwrap_err();
    assert!(err.contains("DOES_NOT_EXIST"));
}

// ---------------------------------------------------------------------
// apply_overlay: full SdkConfig merge tests
// ---------------------------------------------------------------------

#[test]
fn test_apply_overlay_merges_lists_and_overrides_scalars() {
    let base = create_complex_sdk_config();
    let mut derived = create_minimal_sdk_config();
    derived.build_folder = Some("custom-build".to_string());

    let overlay = OverlayConfig {
        gits: Some(GitsOverlay {
            add: vec![new_git(
                "drone-camera",
                "https://example.com/camera.git",
                "main",
            )],
            remove: vec!["application".to_string()],
            modify: vec![],
        }),
        toolchains: None,
        install: None,
        copy_files: None,
        variables: None,
    };

    let merged = apply_overlay(base, derived, &overlay).expect("apply_overlay should succeed");
    assert_eq!(merged.gits.len(), 3);
    assert!(merged.gits.iter().any(|g| g.name == "drone-camera"));
    assert!(!merged.gits.iter().any(|g| g.name == "application"));
    assert_eq!(merged.build_folder.as_deref(), Some("custom-build"));
    assert!(merged.extends.is_none());
}

#[test]
fn test_apply_overlay_rejects_gits_directly_in_derived_sdk_yml() {
    let base = create_complex_sdk_config();
    let mut derived = create_minimal_sdk_config();
    derived.gits = vec![new_git("not-allowed", "https://example.com/x.git", "main")];

    let err = apply_overlay(base, derived, &OverlayConfig::default()).unwrap_err();
    assert!(err.contains("gits:"));
    assert!(err.contains("overlay.yml"));
}

#[test]
fn test_apply_overlay_missing_overlay_is_noop_on_lists() {
    let base = create_complex_sdk_config();
    let derived = create_minimal_sdk_config();
    let base_len = base.gits.len();

    let merged = apply_overlay(base, derived, &OverlayConfig::default())
        .expect("apply_overlay should succeed");
    assert_eq!(merged.gits.len(), base_len);
}

// ---------------------------------------------------------------------
// validate_dependencies tests
// ---------------------------------------------------------------------

#[test]
fn test_validate_dependencies_ok() {
    let config = create_complex_sdk_config();
    assert!(validate_dependencies(&config).is_ok());
}

#[test]
fn test_validate_dependencies_dangling_build_depends_on() {
    let mut config = create_minimal_sdk_config();
    config.gits = vec![GitConfig {
        name: "app".to_string(),
        url: "https://example.com/app.git".to_string(),
        commit: "main".to_string(),
        build_depends_on: Some(vec!["removed-git".to_string()]),
        git_depends_on: None,
        build: None,
        documentation_dir: None,
        python_deps: None,
        group: None,
    }];

    let err = validate_dependencies(&config).unwrap_err();
    assert!(err.contains("app"));
    assert!(err.contains("removed-git"));
}

#[test]
fn test_validate_dependencies_dangling_install_depends_on() {
    let mut config = create_minimal_sdk_config();
    config.install = Some(vec![new_install("c", Some(vec!["missing-install"]))]);

    let err = validate_dependencies(&config).unwrap_err();
    assert!(err.contains("missing-install"));
}

// ---------------------------------------------------------------------
// compute_owned_entries tests
// ---------------------------------------------------------------------

#[test]
fn test_compute_owned_entries_across_all_sections() {
    let overlay = OverlayConfig {
        gits: Some(GitsOverlay {
            add: vec![new_git(
                "drone-camera",
                "https://example.com/camera.git",
                "main",
            )],
            remove: vec!["mcuboot".to_string()],
            modify: vec![GitPatch {
                name: "zephyr".to_string(),
                url: None,
                commit: Some("v4.5.0".to_string()),
                build_depends_on: None,
                git_depends_on: None,
                build: None,
                documentation_dir: None,
                python_deps: None,
                group: None,
            }],
        }),
        toolchains: Some(ToolchainsOverlay {
            add: vec![new_toolchain("gcc-drone")],
            remove: vec![],
            modify: vec![],
        }),
        install: Some(InstallOverlay {
            add: vec![],
            remove: vec![],
            modify: vec![InstallPatch {
                name: "protoc".to_string(),
                depends_on: None,
                sentinel: None,
                commands: None,
            }],
        }),
        copy_files: Some(CopyFilesOverlay {
            add: vec![new_copy_file("patches/drone.patch")],
            remove: vec![],
            modify: vec![],
        }),
        variables: None,
    };

    let owned = compute_owned_entries(&overlay);

    assert_eq!(owned.gits.len(), 2); // drone-camera (add) + zephyr (modify)
    assert!(owned.gits.contains("drone-camera"));
    assert!(owned.gits.contains("zephyr"));
    assert!(!owned.gits.contains("mcuboot")); // remove: doesn't count as "owned"

    assert_eq!(owned.toolchains.len(), 1);
    assert!(owned.toolchains.contains("gcc-drone"));

    assert_eq!(owned.install.len(), 1);
    assert!(owned.install.contains("protoc"));

    assert_eq!(owned.copy_files.len(), 1);
    assert!(owned.copy_files.contains("patches/drone.patch"));
}

#[test]
fn test_compute_owned_entries_default_overlay_is_empty() {
    let owned = compute_owned_entries(&OverlayConfig::default());
    assert!(owned.gits.is_empty());
    assert!(owned.toolchains.is_empty());
    assert!(owned.install.is_empty());
    assert!(owned.copy_files.is_empty());
}
