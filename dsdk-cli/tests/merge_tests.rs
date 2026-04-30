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

//! Integration tests for the merge feature.
//!
//! These tests verify end-to-end merging of multiple SDK target
//! configurations loaded from disk, including fragment post-processing.

mod common;

use common::TestFixture;
use dsdk_cli::config::load_config;
use dsdk_cli::fragment::apply_fragment;
use dsdk_cli::merge::merge_targets;

const TARGET_A_CONFIG: &str = r#"
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

variables:
  CROSS_COMPILE: aarch64-linux-gnu-
  ARCH: arm64

build:
  - make all
"#;

const TARGET_B_CONFIG: &str = r#"
mirror: /tmp/mirror

gits:
  - name: zephyr
    url: https://github.com/zephyrproject-rtos/zephyr.git
    commit: v3.5.0
    build:
      - west build
  - name: hal_stm32
    url: https://github.com/zephyrproject-rtos/hal_stm32.git
    commit: main
    git_depends_on:
      - zephyr

toolchains:
  - name: zephyr-sdk
    url: https://example.com/zephyr-sdk-0.16.tar.xz
    destination: /opt/toolchains/zephyr-sdk

install:
  - name: west
    commands:
      - pip install west

variables:
  ZEPHYR_BASE: zephyr

test:
  - twister -T tests/
"#;

/// Create a manifests tree with two targets under targets/ dir
fn create_manifests_tree(fixture: &TestFixture) {
    fixture.write_file("targets/target-a/sdk.yml", TARGET_A_CONFIG);
    fixture.write_file("targets/target-b/sdk.yml", TARGET_B_CONFIG);
}

#[test]
fn test_merge_two_targets_from_disk() {
    let fixture = TestFixture::new();
    create_manifests_tree(&fixture);

    let cfg_a = load_config(fixture.path().join("targets/target-a/sdk.yml")).unwrap();
    let cfg_b = load_config(fixture.path().join("targets/target-b/sdk.yml")).unwrap();

    let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);

    assert!(
        result.conflicts.is_empty(),
        "conflicts: {:?}",
        result.conflicts
    );

    // All gits combined
    assert_eq!(result.config.gits.len(), 4);
    assert!(result.config.gits.iter().any(|g| g.name == "kernel"));
    assert!(result.config.gits.iter().any(|g| g.name == "zephyr"));

    // Toolchains combined
    let tcs = result.config.toolchains.as_ref().unwrap();
    assert_eq!(tcs.len(), 2);

    // Variables combined
    let vars = result.config.variables.as_ref().unwrap();
    assert_eq!(vars["CROSS_COMPILE"], "aarch64-linux-gnu-");
    assert_eq!(vars["ZEPHYR_BASE"], "zephyr");

    // Install from target-b
    let installs = result.config.install.as_ref().unwrap();
    assert_eq!(installs.len(), 1);
    assert_eq!(installs[0].name, "west");

    // Global targets omitted
    assert!(result.config.build.is_none());
    assert!(result.config.test.is_none());
}

#[test]
fn test_merge_detects_git_conflict() {
    let fixture = TestFixture::new();
    // Both targets define the same git name
    let target_c = r#"
mirror: /tmp/mirror
gits:
  - name: shared-lib
    url: https://example.com/shared-lib.git
    commit: v1.0
"#;
    let target_d = r#"
mirror: /tmp/mirror
gits:
  - name: shared-lib
    url: https://example.com/shared-lib-fork.git
    commit: v2.0
"#;
    fixture.write_file("targets/target-c/sdk.yml", target_c);
    fixture.write_file("targets/target-d/sdk.yml", target_d);

    let cfg_c = load_config(fixture.path().join("targets/target-c/sdk.yml")).unwrap();
    let cfg_d = load_config(fixture.path().join("targets/target-d/sdk.yml")).unwrap();

    let result = merge_targets(&[("target-c", &cfg_c), ("target-d", &cfg_d)], None);

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].section, "gits");
    assert_eq!(result.conflicts[0].key, "shared-lib");
}

#[test]
fn test_merge_with_fragment_post_processing() {
    let fixture = TestFixture::new();
    create_manifests_tree(&fixture);

    // Create a fragment that adds a global build target to the merged result
    let fragment_yaml = r#"
fragment:
  name: merged-build
  description: Adds build target for merged workspace

build:
  - make all-targets
"#;
    fixture.write_file("merge-build.fragment.yml", fragment_yaml);

    let cfg_a = load_config(fixture.path().join("targets/target-a/sdk.yml")).unwrap();
    let cfg_b = load_config(fixture.path().join("targets/target-b/sdk.yml")).unwrap();

    let mut result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);
    assert!(result.conflicts.is_empty());

    // Global build should be None before fragment
    assert!(result.config.build.is_none());

    // Apply fragment
    let fragment =
        dsdk_cli::config::load_fragment(fixture.path().join("merge-build.fragment.yml")).unwrap();
    apply_fragment(&mut result.config, &fragment).unwrap();

    // Now build should be set
    assert!(result.config.build.is_some());
}

#[test]
fn test_merge_mirror_override() {
    let fixture = TestFixture::new();
    create_manifests_tree(&fixture);

    let cfg_a = load_config(fixture.path().join("targets/target-a/sdk.yml")).unwrap();
    let cfg_b = load_config(fixture.path().join("targets/target-b/sdk.yml")).unwrap();

    let custom_mirror = std::path::PathBuf::from("/custom/mirror/path");
    let result = merge_targets(
        &[("target-a", &cfg_a), ("target-b", &cfg_b)],
        Some(&custom_mirror),
    );

    assert_eq!(result.config.mirror, custom_mirror);
}

#[test]
fn test_merge_preserves_dependencies() {
    let fixture = TestFixture::new();
    create_manifests_tree(&fixture);

    let cfg_a = load_config(fixture.path().join("targets/target-a/sdk.yml")).unwrap();
    let cfg_b = load_config(fixture.path().join("targets/target-b/sdk.yml")).unwrap();

    let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);
    assert!(result.conflicts.is_empty());

    // u-boot from target-a should still depend on kernel
    let uboot = result
        .config
        .gits
        .iter()
        .find(|g| g.name == "u-boot")
        .unwrap();
    assert_eq!(
        uboot.build_depends_on.as_ref().unwrap(),
        &vec!["kernel".to_string()]
    );

    // hal_stm32 from target-b should still depend on zephyr
    let hal = result
        .config
        .gits
        .iter()
        .find(|g| g.name == "hal_stm32")
        .unwrap();
    assert_eq!(
        hal.git_depends_on.as_ref().unwrap(),
        &vec!["zephyr".to_string()]
    );
}

#[test]
fn test_merge_result_roundtrip_yaml() {
    let fixture = TestFixture::new();
    create_manifests_tree(&fixture);

    let cfg_a = load_config(fixture.path().join("targets/target-a/sdk.yml")).unwrap();
    let cfg_b = load_config(fixture.path().join("targets/target-b/sdk.yml")).unwrap();

    let result = merge_targets(&[("target-a", &cfg_a), ("target-b", &cfg_b)], None);
    assert!(result.conflicts.is_empty());

    // Serialize and re-parse
    let yaml = serde_yaml::to_string(&result.config).unwrap();
    let output_dir = fixture.create_dir("output");
    std::fs::write(output_dir.join("sdk.yml"), &yaml).unwrap();
    let reloaded = load_config(output_dir.join("sdk.yml")).unwrap();

    assert_eq!(reloaded.gits.len(), result.config.gits.len());
    assert_eq!(
        reloaded.toolchains.as_ref().map(|t| t.len()),
        result.config.toolchains.as_ref().map(|t| t.len())
    );
}

#[test]
fn test_merge_variable_conflict_different_values() {
    let fixture = TestFixture::new();
    let target_e = r#"
mirror: /tmp/mirror
gits:
  - name: repo-e
    url: https://example.com/repo-e.git
    commit: main
variables:
  SHARED_KEY: value_from_e
"#;
    let target_f = r#"
mirror: /tmp/mirror
gits:
  - name: repo-f
    url: https://example.com/repo-f.git
    commit: main
variables:
  SHARED_KEY: value_from_f
"#;
    fixture.write_file("targets/target-e/sdk.yml", target_e);
    fixture.write_file("targets/target-f/sdk.yml", target_f);

    let cfg_e = load_config(fixture.path().join("targets/target-e/sdk.yml")).unwrap();
    let cfg_f = load_config(fixture.path().join("targets/target-f/sdk.yml")).unwrap();

    let result = merge_targets(&[("target-e", &cfg_e), ("target-f", &cfg_f)], None);

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].section, "variables");
    assert_eq!(result.conflicts[0].key, "SHARED_KEY");
}

#[test]
fn test_merge_notes_for_different_mirrors() {
    let fixture = TestFixture::new();
    let target_g = r#"
mirror: /mirror/path-one
gits:
  - name: repo-g
    url: https://example.com/repo-g.git
    commit: main
"#;
    let target_h = r#"
mirror: /mirror/path-two
gits:
  - name: repo-h
    url: https://example.com/repo-h.git
    commit: main
"#;
    fixture.write_file("targets/target-g/sdk.yml", target_g);
    fixture.write_file("targets/target-h/sdk.yml", target_h);

    let cfg_g = load_config(fixture.path().join("targets/target-g/sdk.yml")).unwrap();
    let cfg_h = load_config(fixture.path().join("targets/target-h/sdk.yml")).unwrap();

    let result = merge_targets(&[("target-g", &cfg_g), ("target-h", &cfg_h)], None);

    assert!(result.conflicts.is_empty());
    // First target's mirror wins
    assert_eq!(
        result.config.mirror,
        std::path::PathBuf::from("/mirror/path-one")
    );
    // Should have a note about the different mirrors
    assert!(
        result.notes.iter().any(|n| n.contains("different mirror")),
        "notes: {:?}",
        result.notes
    );
}
