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

//! Integration tests for the manifest fragment system.
//!
//! These tests verify end-to-end fragment loading, application, discovery,
//! and validation using files on disk.

mod common;

use common::TestFixture;
use dsdk_cli::config::{load_config, load_fragment};
use dsdk_cli::fragment::{
    apply_fragment, apply_fragments, discover_fragments_in_dir, validate_merged_config,
};

/// Helper to create a test workspace with a base sdk.yml and optional fragments
fn create_test_workspace(fixture: &TestFixture, base_yaml: &str) {
    fixture.write_file("sdk.yml", base_yaml);
}

fn create_fragment_file(fixture: &TestFixture, dir: &str, name: &str, content: &str) {
    let path = format!("{}/{}", dir, name);
    fixture.write_file(&path, content);
}

const BASE_CONFIG: &str = r#"
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
    build:
      - make u-boot

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

#[test]
fn test_fragment_file_round_trip() {
    let fixture = TestFixture::new();

    create_fragment_file(
        &fixture,
        ".",
        "custom.fragment.yml",
        r#"
fragment:
  name: custom-override
  description: "Test fragment for round-trip"

gits:
  - name: kernel
    commit: v6.5
"#,
    );

    let frag = load_fragment(fixture.path().join("custom.fragment.yml")).unwrap();
    assert_eq!(
        frag.fragment.as_ref().unwrap().name.as_deref(),
        Some("custom-override")
    );
    assert_eq!(
        frag.fragment.as_ref().unwrap().description.as_deref(),
        Some("Test fragment for round-trip")
    );
    assert_eq!(frag.gits.as_ref().unwrap().len(), 1);
    assert_eq!(frag.gits.as_ref().unwrap()[0].name, "kernel");
    assert_eq!(
        frag.gits.as_ref().unwrap()[0].commit.as_deref(),
        Some("v6.5")
    );
    // URL should be None (not overridden)
    assert!(frag.gits.as_ref().unwrap()[0].url.is_none());
}

#[test]
fn test_discover_and_apply_fragments_from_dir() {
    let fixture = TestFixture::new();

    create_test_workspace(&fixture, BASE_CONFIG);
    fixture.create_dir("fragments");

    // Create two fragments that should be applied alphabetically
    create_fragment_file(
        &fixture,
        "fragments",
        "01-override-kernel.fragment.yml",
        r#"
fragment:
  name: override-kernel
gits:
  - name: kernel
    commit: v6.3
"#,
    );

    create_fragment_file(
        &fixture,
        "fragments",
        "02-add-repo.fragment.yml",
        r#"
fragment:
  name: add-custom-repo
gits:
  - name: my-app
    url: https://github.com/me/app.git
    commit: main
"#,
    );

    // Also create a non-fragment file that should be ignored
    fixture.write_file("fragments/readme.md", "# Not a fragment");

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment_dir = fixture.path().join("fragments");
    let paths = discover_fragments_in_dir(&fragment_dir).unwrap();

    assert_eq!(paths.len(), 2);
    assert!(paths[0]
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("01-"));
    assert!(paths[1]
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("02-"));

    let fragments: Vec<_> = paths.iter().map(|p| load_fragment(p).unwrap()).collect();

    apply_fragments(&mut config, &fragments).unwrap();

    // Kernel commit should be v6.3 (from first fragment)
    let kernel = config.gits.iter().find(|g| g.name == "kernel").unwrap();
    assert_eq!(kernel.commit, "v6.3");
    // URL preserved
    assert_eq!(kernel.url, "https://github.com/torvalds/linux.git");

    // my-app added by second fragment
    assert_eq!(config.gits.len(), 3);
    let app = config.gits.iter().find(|g| g.name == "my-app").unwrap();
    assert_eq!(app.url, "https://github.com/me/app.git");
}

#[test]
fn test_fragment_with_remove_and_add() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    create_fragment_file(
        &fixture,
        ".",
        "slim.fragment.yml",
        r#"
fragment:
  name: slim-build
  description: "Remove u-boot, override kernel, remove cmake"

remove_gits:
  - u-boot

gits:
  - name: kernel
    commit: my-feature-branch

remove_install:
  - cmake

remove_copy_files:
  - dest: scripts/setup.sh

variables:
  CUSTOM_FLAG: "1"
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("slim.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    // u-boot removed
    assert_eq!(config.gits.len(), 1);
    assert_eq!(config.gits[0].name, "kernel");
    assert_eq!(config.gits[0].commit, "my-feature-branch");

    // cmake removed, ninja kept
    let installs = config.install.as_ref().unwrap();
    assert_eq!(installs.len(), 1);
    assert_eq!(installs[0].name, "ninja");

    // copy_files entry removed
    assert!(config.copy_files.as_ref().unwrap().is_empty());

    // Variable added
    let vars = config.variables.as_ref().unwrap();
    assert_eq!(vars["CUSTOM_FLAG"], "1");
    assert_eq!(vars["ARCH"], "arm64"); // preserved
}

#[test]
fn test_fragment_toolchain_deep_merge() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    create_fragment_file(
        &fixture,
        ".",
        "toolchain.fragment.yml",
        r#"
toolchains:
  - name: arm-gcc
    url: https://example.com/arm-gcc-13.tar.xz
    sha256: newhash123
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("toolchain.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    let tc = &config.toolchains.as_ref().unwrap()[0];
    // URL overridden
    assert_eq!(tc.url, "https://example.com/arm-gcc-13.tar.xz");
    // SHA256 added
    assert_eq!(tc.sha256.as_deref(), Some("newhash123"));
    // Destination preserved from base
    assert_eq!(tc.destination, "/opt/toolchains/arm-gcc");
    // OS preserved
    assert_eq!(tc.os.as_deref(), Some("linux"));
}

#[test]
fn test_fragment_global_target_replacement() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    create_fragment_file(
        &fixture,
        ".",
        "targets.fragment.yml",
        r#"
build:
  commands:
    - make custom-build
  depends_on:
    - sdk-envsetup
test: make custom-test
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("targets.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    // Build replaced with new format including depends_on
    let build = config.build.as_ref().unwrap();
    assert_eq!(build.commands(), &["make custom-build"]);
    assert_eq!(build.depends_on().unwrap(), &["sdk-envsetup"]);

    // Test replaced
    assert_eq!(
        config.test.as_ref().unwrap().commands(),
        &["make custom-test"]
    );

    // Clean NOT replaced (not in fragment)
    assert_eq!(config.clean.as_ref().unwrap().commands(), &["make clean"]);
}

#[test]
fn test_multiple_fragments_precedence() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    // Fragment 1: set kernel to v6.3
    create_fragment_file(
        &fixture,
        ".",
        "frag1.fragment.yml",
        r#"
gits:
  - name: kernel
    commit: v6.3
variables:
  CROSS_COMPILE: arm-none-eabi-
"#,
    );

    // Fragment 2: set kernel to v6.5 (should win)
    create_fragment_file(
        &fixture,
        ".",
        "frag2.fragment.yml",
        r#"
gits:
  - name: kernel
    commit: v6.5
variables:
  NEW_VAR: hello
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let frag1 = load_fragment(fixture.path().join("frag1.fragment.yml")).unwrap();
    let frag2 = load_fragment(fixture.path().join("frag2.fragment.yml")).unwrap();

    apply_fragments(&mut config, &[frag1, frag2]).unwrap();

    // Last fragment wins for kernel commit
    let kernel = config.gits.iter().find(|g| g.name == "kernel").unwrap();
    assert_eq!(kernel.commit, "v6.5");

    // Variables: both fragments' changes accumulated
    let vars = config.variables.as_ref().unwrap();
    assert_eq!(vars["CROSS_COMPILE"], "arm-none-eabi-"); // from frag1
    assert_eq!(vars["NEW_VAR"], "hello"); // from frag2
    assert_eq!(vars["ARCH"], "arm64"); // preserved from base
}

#[test]
fn test_validation_catches_dangling_deps_after_fragment() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    // Remove kernel but u-boot depends on it
    create_fragment_file(
        &fixture,
        ".",
        "broken.fragment.yml",
        r#"
remove_gits:
  - kernel
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("broken.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    let warnings = validate_merged_config(&config);
    assert!(!warnings.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.contains("build_depends_on 'kernel'")));
}

#[test]
fn test_fragment_with_numeric_commit() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    // Test that numeric commits (like 2025.05) parse correctly in fragments
    create_fragment_file(
        &fixture,
        ".",
        "numeric.fragment.yml",
        r#"
gits:
  - name: kernel
    commit: 2025.05
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("numeric.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    let kernel = config.gits.iter().find(|g| g.name == "kernel").unwrap();
    assert_eq!(kernel.commit, "2025.05");
}

#[test]
fn test_empty_fragment_is_noop() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    create_fragment_file(
        &fixture,
        ".",
        "empty.fragment.yml",
        r#"
fragment:
  name: empty
  description: "Does nothing"
"#,
    );

    let config_before = load_config(fixture.path().join("sdk.yml")).unwrap();
    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("empty.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    // Everything should be unchanged
    assert_eq!(config.gits.len(), config_before.gits.len());
    assert_eq!(config.mirror, config_before.mirror);
    for (a, b) in config.gits.iter().zip(config_before.gits.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.url, b.url);
        assert_eq!(a.commit, b.commit);
    }
}

#[test]
fn test_fragment_error_on_invalid_yaml() {
    let fixture = TestFixture::new();
    fixture.write_file("bad.fragment.yml", "this is: not: valid: yaml: [");

    let result = load_fragment(fixture.path().join("bad.fragment.yml"));
    assert!(result.is_err());
}

#[test]
fn test_fragment_error_on_missing_file() {
    let result = load_fragment("/nonexistent/path/fragment.yml");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Fragment file not found"));
}

#[test]
fn test_add_new_toolchain_via_fragment() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    create_fragment_file(
        &fixture,
        ".",
        "newtc.fragment.yml",
        r#"
toolchains:
  - name: riscv-gcc
    url: https://example.com/riscv-gcc.tar.xz
    destination: /opt/toolchains/riscv-gcc
    os: linux
    arch: x86_64
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("newtc.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    let tcs = config.toolchains.as_ref().unwrap();
    assert_eq!(tcs.len(), 2);
    assert_eq!(tcs[1].get_name(), "riscv-gcc");
}

#[test]
fn test_fragment_install_add_and_modify() {
    let fixture = TestFixture::new();
    create_test_workspace(&fixture, BASE_CONFIG);

    create_fragment_file(
        &fixture,
        ".",
        "install.fragment.yml",
        r#"
install:
  - name: ninja
    commands:
      - pip install ninja
  - name: new-tool
    commands:
      - ./install-new-tool.sh
    depends_on:
      - ninja
"#,
    );

    let mut config = load_config(fixture.path().join("sdk.yml")).unwrap();
    let fragment = load_fragment(fixture.path().join("install.fragment.yml")).unwrap();
    apply_fragment(&mut config, &fragment).unwrap();

    let installs = config.install.as_ref().unwrap();
    assert_eq!(installs.len(), 3); // ninja (modified), cmake (kept), new-tool (added)

    let ninja = installs.iter().find(|i| i.name == "ninja").unwrap();
    assert_eq!(
        ninja.commands.as_ref().unwrap(),
        &["pip install ninja".to_string()]
    );

    let new_tool = installs.iter().find(|i| i.name == "new-tool").unwrap();
    assert_eq!(
        new_tool.depends_on.as_ref().unwrap(),
        &["ninja".to_string()]
    );
}
