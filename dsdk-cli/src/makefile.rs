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

use dsdk_cli::workspace::get_current_workspace;
use dsdk_cli::{config, messages, vscode_tasks_manager};

const WORKSPACE_VARIABLE: &str = "WORKSPACE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))";

/// Generate a Makefile from the SDK configuration
pub(crate) fn handle_makefile_command(no_dividers: bool, ninja: bool) {
    // Must be run from within a workspace
    let workspace_path = match get_current_workspace() {
        Ok(path) => path,
        Err(e) => {
            messages::error(&format!("Error: {}", e));
            return;
        }
    };

    let output_path = workspace_path.join("Makefile");

    // Use sdk.yml from workspace root
    let config_path = workspace_path.join("sdk.yml");
    if !config_path.exists() {
        messages::error(&format!(
            "sdk.yml not found in workspace root: {}",
            workspace_path.display()
        ));
        messages::error("The workspace may be corrupted. Try running 'cim init' to reinitialize.");
        return;
    }

    let mut sdk_config = match config::load_config(&config_path) {
        Ok(config) => config,
        Err(e) => {
            messages::error(&format!("Error loading config: {}", e));
            return;
        }
    };

    // Resolve effective no_dividers: CLI flag overrides user config
    let mut effective_no_dividers = false;

    // Load and apply user config overrides if present
    match config::UserConfig::load() {
        Ok(Some(user_config)) => {
            user_config.apply_to_sdk_config(&mut sdk_config, false);
            if let Some(config_no_dividers) = user_config.no_dividers {
                effective_no_dividers = config_no_dividers;
            }
        }
        Ok(None) => {}
        Err(e) => {
            messages::error(&format!("Warning: Failed to load user config: {}", e));
        }
    }

    // CLI --no-dividers always wins
    if no_dividers {
        effective_no_dividers = true;
    }

    let dividers = !effective_no_dividers;
    let makefile = generate_makefile_content(&sdk_config, dividers, ninja);

    match std::fs::write(&output_path, makefile) {
        Ok(_) => messages::success(&format!("Makefile written to {}", output_path.display())),
        Err(e) => messages::error(&format!("Failed to write Makefile: {}", e)),
    }

    // Generate .cim/ directory (toolchain.mk, per-repo stubs, generated.mk)
    generate_cim_directory(&workspace_path, &sdk_config);

    // Optionally generate .cim/build.ninja for parallel repo builds
    if ninja {
        generate_ninja_file(&workspace_path, &sdk_config);
    }

    // NEW: Generate VS Code tasks.json
    if let Err(e) = vscode_tasks_manager::generate_tasks_json(&workspace_path, &output_path) {
        messages::info(&format!("Could not generate VS Code tasks.json: {}", e));
    }
}

/// Generate a section divider comment banner for a Makefile
fn makefile_divider(title: &str) -> String {
    format!(
        "################################################################################\n\
         # {}\n\
         ################################################################################\n",
        title
    )
}

/// Generate the content of the Makefile from SDK configuration
pub(crate) fn generate_makefile_content<T: config::SdkConfigCore>(
    sdk_config: &T,
    dividers: bool,
    ninja: bool,
) -> String {
    let mut makefile = String::new();

    if dividers {
        makefile.push_str(&makefile_divider("Workspace and manifest variables"));
    }

    makefile.push_str(WORKSPACE_VARIABLE);
    makefile.push_str("\n\n");

    // Emit manifest variables as Make ?= assignments so host env vars override them
    let vars: std::collections::HashMap<String, String> =
        if let Some(raw_vars) = sdk_config.variables() {
            dsdk_cli::workspace::resolve_variables(raw_vars)
        } else {
            std::collections::HashMap::new()
        };

    if !vars.is_empty() {
        makefile
            .push_str("# Manifest variables — override with host env vars before invoking make\n");
        // Sort for deterministic output
        let mut sorted: Vec<_> = vars.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (key, value) in sorted {
            makefile.push_str(&format!("{} ?= {}\n", key, value));
        }
        makefile.push('\n');
    }

    // When --ninja was used to generate, emit variables for auto-detection
    if ninja {
        makefile.push_str("# Ninja auto-detection: sdk-build uses ninja when available\n");
        makefile.push_str("NINJA := $(shell command -v ninja 2>/dev/null)\n");
        makefile.push_str("NINJA_BUILD_FILE := $(WORKSPACE)/.cim/build.ninja\n\n");
    }

    // Add makefile includes after variables so included files can reference them.
    //
    // New path: if `overlays:` is declared in sdk.yml, use the .cim/ system.
    // .cim/generated.mk is the single entry point that includes toolchain.mk,
    // explicit overlays, and any repo-provided .config/cim/cim.mk files.
    //
    // Legacy path: if only `makefile_include:` is present (no overlays:), emit
    // the raw -include lines as before so existing manifests keep working.
    let has_overlays = sdk_config
        .overlays()
        .as_ref()
        .map(|o| !o.is_empty())
        .unwrap_or(false);

    if has_overlays {
        if dividers {
            makefile.push_str(&makefile_divider("Makefile includes (.cim/ system)"));
        }
        makefile.push_str(
            "# Includes toolchain vars, overlays, and repo-provided .config/cim/cim.mk files.\n",
        );
        makefile.push_str("-include .cim/generated.mk\n\n");
    } else if let Some(makefile_includes) = sdk_config.makefile_include() {
        if !makefile_includes.is_empty() {
            if dividers {
                makefile.push_str(&makefile_divider("Makefile includes"));
            }
            for include_line in makefile_includes {
                makefile.push_str(&format!("-{}\n", include_line));
            }
            makefile.push('\n');
        }
    }

    if dividers {
        makefile.push_str(&makefile_divider("High-level SDK targets"));
    }

    // Add .PHONY declarations
    let mut phony_targets = vec!["all"];

    // Add sdk-envsetup to PHONY if envsetup commands exist
    if let Some(_envsetup_target) = sdk_config.envsetup() {
        phony_targets.push("sdk-envsetup");
    }

    // Add sdk-test to PHONY if test commands exist
    if let Some(_test_target) = sdk_config.test() {
        phony_targets.push("sdk-test");
    }

    // Add sdk-clean to PHONY (always add it, even if no commands)
    phony_targets.push("sdk-clean");

    // Add sdk-build to PHONY (always add it, even if no commands)
    phony_targets.push("sdk-build");

    // Add sdk-flash to PHONY (always add it, even if no commands)
    phony_targets.push("sdk-flash");

    // Add install targets to PHONY if install section exists
    if let Some(install_configs) = sdk_config.install() {
        if !install_configs.is_empty() {
            phony_targets.push("install-all");
        }
    }

    makefile.push_str(&format!(".PHONY: {}\n", phony_targets.join(" ")));

    // Add 'all' target that depends on sdk-build and sdk-test
    let mut all_deps = vec!["sdk-build"];
    if sdk_config.test().is_some() {
        all_deps.push("sdk-test");
    }
    makefile.push_str(&format!("all: {}\n\n", all_deps.join(" ")));

    // Add sdk-envsetup target if envsetup commands exist
    if let Some(envsetup_target) = sdk_config.envsetup() {
        add_envsetup_target(&mut makefile, envsetup_target);
    }

    // Add sdk-test target if test commands exist
    if let Some(test_target) = sdk_config.test() {
        add_test_target(&mut makefile, test_target);
    }

    // Add sdk-clean target (always create, fallback if missing)
    match sdk_config.clean() {
        Some(clean_target) => {
            add_clean_target(&mut makefile, clean_target);
        }
        _ => {
            makefile.push_str("sdk-clean:\n\t@echo \"No clean commands defined in sdk.yml\"\n\n");
        }
    }

    // Add sdk-build target (always create, fallback if missing)
    match sdk_config.build() {
        Some(build_target) => {
            add_build_target(&mut makefile, build_target, ninja);
        }
        _ => {
            makefile.push_str("sdk-build:\n\t@echo \"No build commands defined in sdk.yml\"\n\n");
        }
    }

    // Add sdk-flash target (always create, fallback if missing)
    match sdk_config.flash() {
        Some(flash_target) => {
            add_flash_target(&mut makefile, flash_target);
        }
        _ => {
            makefile.push_str("sdk-flash:\n\t@echo \"No flash commands defined in sdk.yml\"\n\n");
        }
    }

    // Add install-all target if install section exists
    let has_install = match sdk_config.install() {
        Some(configs) => !configs.is_empty(),
        None => false,
    };

    if has_install && dividers {
        makefile.push_str(&makefile_divider("Installation targets"));
    }

    if let Some(install_configs) = sdk_config.install() {
        if !install_configs.is_empty() {
            add_install_all_target(&mut makefile, install_configs);
        }
    }

    // Add individual install targets
    if let Some(install_configs) = sdk_config.install() {
        for install in install_configs {
            add_install_target(&mut makefile, install);
        }
    }

    // Add individual git targets
    if !sdk_config.gits().is_empty() && dividers {
        makefile.push_str(&makefile_divider("Git repository targets"));
    }
    for git in sdk_config.gits() {
        add_makefile_target(&mut makefile, git);
    }

    makefile
}

/// Render a command string for inclusion in a Makefile recipe.
///
/// Converts manifest variable references `${{ VAR }}` to Make variable
/// references `$(VAR)`.  Other content (e.g. existing `$(MAKE)`) is left
/// unchanged.
pub(crate) fn render_command_for_makefile(cmd: &str) -> String {
    let mut result = cmd.to_string();
    let mut search_start = 0;
    while let Some(open) = result[search_start..].find("${{") {
        let open_abs = search_start + open;
        let after_open = open_abs + 3;
        if let Some(close_rel) = result[after_open..].find("}}") {
            let close_abs = after_open + close_rel;
            let var_name = result[after_open..close_abs].trim();
            let make_ref = format!("$({})", var_name);
            let token_end = close_abs + 2;
            result.replace_range(open_abs..token_end, &make_ref);
            search_start = open_abs + make_ref.len();
        } else {
            break;
        }
    }
    result
}

/// Add a single target to the Makefile
pub(crate) fn add_makefile_target(makefile: &mut String, git: &config::GitConfig) {
    // Add .PHONY declaration for this target
    makefile.push_str(&format!(".PHONY: {}\n", git.name));

    // Add target with dependencies
    let dep_str = if let Some(deps) = &git.build_depends_on {
        deps.join(" ")
    } else {
        String::new()
    };

    if dep_str.is_empty() {
        makefile.push_str(&format!("{}:\n", git.name));
    } else {
        makefile.push_str(&format!("{}: {}\n", git.name, dep_str));
    }

    // Add build commands
    if let Some(build_cmds) = &git.build {
        for cmd in build_cmds {
            let rendered = render_command_for_makefile(cmd);
            let trimmed = rendered.trim();
            if trimmed.starts_with('#') {
                // Write as a Makefile comment (with tab like other commands)
                makefile.push_str(&format!(
                    "\t#{}\n",
                    trimmed.strip_prefix('#').unwrap().trim_start()
                ));
            } else {
                makefile.push_str(&format!("\t{}\n", rendered));
            }
        }
    } else {
        makefile.push_str(&format!("\t@echo Building {}\n", git.name));
    }
    makefile.push('\n');
}

/// Add the sdk-envsetup target to the Makefile
pub(crate) fn add_envsetup_target(makefile: &mut String, envsetup_target: &config::SdkTarget) {
    // Add target with dependencies
    if let Some(deps) = envsetup_target.depends_on() {
        makefile.push_str(&format!("sdk-envsetup: {}\n", deps.join(" ")));
    } else {
        makefile.push_str("sdk-envsetup:\n");
    }

    for command in envsetup_target.commands() {
        let rendered = render_command_for_makefile(command);
        let trimmed = rendered.trim();

        // Skip comment lines (starting with #)
        if trimmed.starts_with('#') {
            // Write as a Makefile comment (with tab like other commands)
            makefile.push_str(&format!(
                "\t#{}\n",
                trimmed.strip_prefix('#').unwrap().trim_start()
            ));
            continue;
        }

        // Handle echo commands with @ prefix (like build commands)
        if trimmed.starts_with('@') {
            // Just pass through the @ command as-is, it's already properly formatted
            makefile.push_str(&format!("\t{}\n", trimmed));
            continue;
        }

        // Add regular command
        makefile.push_str(&format!("\t{}\n", rendered));
    }

    makefile.push('\n');
}

/// Add the sdk-test target to the Makefile
pub(crate) fn add_test_target(makefile: &mut String, test_target: &config::SdkTarget) {
    // Add target with dependencies
    if let Some(deps) = test_target.depends_on() {
        makefile.push_str(&format!("sdk-test: {}\n", deps.join(" ")));
    } else {
        makefile.push_str("sdk-test:\n");
    }

    for command in test_target.commands() {
        let rendered = render_command_for_makefile(command);
        let trimmed = rendered.trim();

        // Skip comment lines (starting with #)
        if trimmed.starts_with('#') {
            // Write as a Makefile comment (with tab like other commands)
            makefile.push_str(&format!(
                "\t#{}\n",
                trimmed.strip_prefix('#').unwrap().trim_start()
            ));
            continue;
        }

        // Handle echo commands with @ prefix (like build commands)
        if trimmed.starts_with('@') {
            // Just pass through the @ command as-is, it's already properly formatted
            makefile.push_str(&format!("\t{}\n", trimmed));
            continue;
        }

        // Add regular command
        makefile.push_str(&format!("\t{}\n", rendered));
    }

    makefile.push('\n');
}

/// Add the sdk-clean target to the Makefile
pub(crate) fn add_clean_target(makefile: &mut String, clean_target: &config::SdkTarget) {
    // Add target with dependencies
    if let Some(deps) = clean_target.depends_on() {
        makefile.push_str(&format!("sdk-clean: {}\n", deps.join(" ")));
    } else {
        makefile.push_str("sdk-clean:\n");
    }

    for command in clean_target.commands() {
        let rendered = render_command_for_makefile(command);
        let trimmed = rendered.trim();

        // Skip comment lines (starting with #)
        if trimmed.starts_with('#') {
            // Write as a Makefile comment (with tab like other commands)
            makefile.push_str(&format!(
                "\t#{}\n",
                trimmed.strip_prefix('#').unwrap().trim_start()
            ));
            continue;
        }

        // Handle echo commands with @ prefix (like build commands)
        if trimmed.starts_with('@') {
            // Just pass through the @ command as-is, it's already properly formatted
            makefile.push_str(&format!("\t{}\n", trimmed));
            continue;
        }

        // Add regular command
        makefile.push_str(&format!("\t{}\n", rendered));
    }

    makefile.push('\n');
}

/// Add the sdk-build target to the Makefile.
///
/// When `ninja` is true, the recipe auto-detects ninja at make-time:
/// - If ninja + `.cim/build.ninja` are present: delegates entirely to ninja,
///   which handles the full dependency graph and runs independent repos in
///   parallel. No Make prerequisites — ninja owns the build graph in this path.
/// - Fallback: runs git dep targets sequentially, then sdk-build commands.
///   Deps are emitted explicitly so the fallback is self-contained.
///
/// When `ninja` is false: standard Make behavior with prerequisites.
pub(crate) fn add_build_target(
    makefile: &mut String,
    build_target: &config::SdkTarget,
    ninja: bool,
) {
    if ninja {
        // In ninja mode, sdk-build has NO Make prerequisites.
        // Ninja handles the full dependency graph when available.
        // The fallback path calls git dep targets explicitly.
        makefile.push_str("sdk-build:\n");
        makefile.push_str("ifneq ($(NINJA),)\n");
        makefile.push_str("ifneq ($(wildcard $(NINJA_BUILD_FILE)),)\n");
        makefile.push_str("\t@echo \"[cim] Using ninja for parallel repo builds\"\n");
        makefile.push_str("\t$(NINJA) -f $(NINJA_BUILD_FILE) sdk-build\n");
        makefile.push_str("else\n");
        // Sequential fallback: call each dep target explicitly
        if let Some(deps) = build_target.depends_on() {
            for dep in deps {
                makefile.push_str(&format!("\t$(MAKE) {}\n", dep));
            }
        }
        for command in build_target.commands() {
            let rendered = render_command_for_makefile(command);
            makefile.push_str(&format!("\t{}\n", rendered.trim()));
        }
        makefile.push_str("endif\n");
        makefile.push_str("else\n");
        // Same sequential fallback when ninja binary is not installed
        if let Some(deps) = build_target.depends_on() {
            for dep in deps {
                makefile.push_str(&format!("\t$(MAKE) {}\n", dep));
            }
        }
        for command in build_target.commands() {
            let rendered = render_command_for_makefile(command);
            makefile.push_str(&format!("\t{}\n", rendered.trim()));
        }
        makefile.push_str("endif\n");
    } else {
        // Standard Make: declare prerequisites so Make runs them before the recipe.
        if let Some(deps) = build_target.depends_on() {
            makefile.push_str(&format!("sdk-build: {}\n", deps.join(" ")));
        } else {
            makefile.push_str("sdk-build:\n");
        }

        for command in build_target.commands() {
            let rendered = render_command_for_makefile(command);
            let trimmed = rendered.trim();

            // Skip comment lines (starting with #)
            if trimmed.starts_with('#') {
                // Write as a Makefile comment (with tab like other commands)
                makefile.push_str(&format!(
                    "\t#{}\n",
                    trimmed.strip_prefix('#').unwrap().trim_start()
                ));
                continue;
            }

            // Handle echo commands with @ prefix (like build commands)
            if trimmed.starts_with('@') {
                // Just pass through the @ command as-is, it's already properly formatted
                makefile.push_str(&format!("\t{}\n", trimmed));
                continue;
            }

            // Add regular command
            makefile.push_str(&format!("\t{}\n", rendered));
        }
    }

    makefile.push('\n');
}

/// Add the sdk-flash target to the Makefile
pub(crate) fn add_flash_target(makefile: &mut String, flash_target: &config::SdkTarget) {
    // Add target with dependencies
    if let Some(deps) = flash_target.depends_on() {
        makefile.push_str(&format!("sdk-flash: {}\n", deps.join(" ")));
    } else {
        makefile.push_str("sdk-flash:\n");
    }

    for command in flash_target.commands() {
        let rendered = render_command_for_makefile(command);
        let trimmed = rendered.trim();

        // Skip comment lines (starting with #)
        if trimmed.starts_with('#') {
            // Write as a Makefile comment (with tab like other commands)
            makefile.push_str(&format!(
                "\t#{}\n",
                trimmed.strip_prefix('#').unwrap().trim_start()
            ));
            continue;
        }

        // Handle echo commands with @ prefix (like build commands)
        if trimmed.starts_with('@') {
            // Just pass through the @ command as-is, it's already properly formatted
            makefile.push_str(&format!("\t{}\n", trimmed));
            continue;
        }

        // Add regular command
        makefile.push_str(&format!("\t{}\n", rendered));
    }

    makefile.push('\n');
}

/// Add install-all target that depends on all install targets
pub(crate) fn add_install_all_target(makefile: &mut String, installs: &[config::InstallConfig]) {
    let all_targets: Vec<_> = installs
        .iter()
        .map(|i| format!("install-{}", i.name))
        .collect();
    makefile.push_str(&format!("install-all: {}\n", all_targets.join(" ")));
    makefile.push_str("\t@echo 'All installations complete'\n\n");
}

/// Check if a line contains shell control structures that should be treated as a complete statement
pub(crate) fn contains_complete_control_structure(line: &str) -> bool {
    let trimmed = line.trim();

    // Single-line if/then/else/fi - complete on one line
    if trimmed.contains("; then") && trimmed.contains("; fi") {
        return true;
    }

    // Single-line while/for with do/done
    if (trimmed.contains("; do") && trimmed.contains("; done"))
        || (trimmed.starts_with("while ") && trimmed.contains("; done"))
        || (trimmed.starts_with("for ") && trimmed.contains("; done"))
    {
        return true;
    }

    false
}

/// Check if a line is a shell control structure keyword that shouldn't have semicolon added after it
/// Returns true for keywords that open or continue a block (then, else, elif, do)
/// Returns false for keywords that close a block (fi, done, esac) - these need semicolons
pub(crate) fn is_shell_control_keyword(line: &str) -> bool {
    let trimmed = line.trim();

    // Control structure keywords that should not have semicolons after them
    // (opening/continuing keywords, not closing ones)
    trimmed == "then"
        || trimmed == "else"
        || trimmed == "elif"
        || trimmed == "do"
        || trimmed.starts_with("then ")
        || trimmed.starts_with("else ")
        || trimmed.starts_with("elif ")
        || trimmed.starts_with("do ")
        || trimmed.ends_with("; then")
        || trimmed.ends_with("; do")
}

/// Check if commands contain shell control structures (if/while/for/case blocks)
pub(crate) fn has_shell_control_structure(commands: &[String]) -> bool {
    commands.iter().any(|cmd| {
        let trimmed = cmd.trim();
        trimmed.starts_with("if ")
            || trimmed.starts_with("while ")
            || trimmed.starts_with("for ")
            || trimmed.starts_with("case ")
            || trimmed == "then"
            || trimmed == "else"
            || trimmed == "elif"
            || trimmed == "fi"
            || trimmed == "do"
            || trimmed == "done"
            || trimmed == "esac"
            || trimmed.ends_with("; then")
            || trimmed.ends_with("; do")
    })
}

/// Add a single install target to the Makefile
pub(crate) fn add_install_target(makefile: &mut String, install: &config::InstallConfig) {
    // Add .PHONY declaration
    makefile.push_str(&format!(".PHONY: install-{}\n", install.name));

    // Add target with dependencies
    let dep_str = if let Some(deps) = &install.depends_on {
        deps.iter()
            .map(|d| format!("install-{}", d))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };

    if dep_str.is_empty() {
        makefile.push_str(&format!("install-{}:\n", install.name));
    } else {
        makefile.push_str(&format!("install-{}: {}\n", install.name, dep_str));
    }

    // If sentinel is specified, wrap commands in check
    if let Some(sentinel) = &install.sentinel {
        makefile.push_str(&format!("\t@if [ ! -f {} ]; then \\\n", sentinel));
        makefile.push_str(&format!("\t  echo 'Installing {}...'; \\\n", install.name));

        if let Some(build_cmds) = &install.commands {
            // Check if commands contain control structures
            let has_control_structure = has_shell_control_structure(build_cmds);

            if has_control_structure {
                // Wrap entire script block in a subshell to preserve control structures
                makefile.push_str("\t  ( \\\n");

                for cmd in build_cmds {
                    let rendered = render_command_for_makefile(cmd);
                    let trimmed = rendered.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        // Check if this is a complete single-line control structure
                        if contains_complete_control_structure(trimmed) {
                            // Single-line if/then/else/fi or while/for - add semicolon at end
                            if !trimmed.ends_with(';') {
                                makefile.push_str(&format!("\t    {}; \\\n", rendered));
                            } else {
                                makefile.push_str(&format!("\t    {} \\\n", rendered));
                            }
                        } else {
                            // Multi-line control structure or regular command
                            // Add semicolons for proper shell syntax, but not for control keywords
                            let needs_semicolon = !is_shell_control_keyword(trimmed)
                                && !trimmed.ends_with(';')
                                && !trimmed.ends_with('{')
                                && !trimmed.ends_with('\\');

                            if needs_semicolon {
                                makefile.push_str(&format!("\t    {}; \\\n", rendered));
                            } else {
                                makefile.push_str(&format!("\t    {} \\\n", rendered));
                            }
                        }
                    }
                }

                makefile.push_str("\t  ) && \\\n");
            } else {
                // No control structures - use original && logic
                for cmd in build_cmds {
                    let rendered = render_command_for_makefile(cmd);
                    let trimmed = rendered.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        // Wrap commands containing 'cd' in subshells to avoid affecting subsequent commands
                        if trimmed.contains(" cd ") || trimmed.starts_with("cd ") {
                            makefile.push_str(&format!("\t  ({}) && \\\n", rendered));
                        } else {
                            makefile.push_str(&format!("\t  {} && \\\n", rendered));
                        }
                    }
                }
            }

            // Create directory for sentinel file if needed, then create sentinel
            if let Some(sentinel_dir) = std::path::Path::new(sentinel).parent() {
                if sentinel_dir != std::path::Path::new("") {
                    makefile.push_str(&format!("\t  mkdir -p {} && \\\n", sentinel_dir.display()));
                }
            }
            makefile.push_str(&format!("\t  touch {} && \\\n", sentinel));
            makefile.push_str(&format!(
                "\t  echo '{} installed successfully'; \\\n",
                install.name
            ));
        }
        makefile.push_str("\telse \\\n");
        makefile.push_str(&format!(
            "\t  echo '{} already installed (sentinel: {})'; \\\n",
            install.name, sentinel
        ));
        makefile.push_str("\tfi\n\n");
    } else {
        // No sentinel, just run commands
        if let Some(build_cmds) = &install.commands {
            for cmd in build_cmds {
                let rendered = render_command_for_makefile(cmd);
                let trimmed = rendered.trim();
                if trimmed.starts_with('#') {
                    makefile.push_str(&format!(
                        "\t#{}\n",
                        trimmed.strip_prefix('#').unwrap().trim_start()
                    ));
                } else {
                    makefile.push_str(&format!("\t{}\n", rendered));
                }
            }
        }
        makefile.push('\n');
    }
}

// ─── .cim/ directory generation ──────────────────────────────────────────────

/// Generate the `.cim/` directory in the workspace with:
///   - `toolchain.mk`  — single source of truth for toolchain variables
///   - `<repo>.mk`     — per-repo generated stubs (only for repos with no
///                       `.config/cim/cim.mk` and no registered overlay)
///   - `generated.mk`  — single include index for the root Makefile
fn generate_cim_directory(workspace_path: &std::path::Path, sdk_config: &config::SdkConfig) {
    let cim_dir = workspace_path.join(".cim");
    if let Err(e) = std::fs::create_dir_all(&cim_dir) {
        messages::error(&format!("Failed to create .cim/ directory: {}", e));
        return;
    }

    // 1. toolchain.mk
    let toolchain_content = generate_toolchain_mk_content(sdk_config);
    let toolchain_path = cim_dir.join("toolchain.mk");
    if let Err(e) = std::fs::write(&toolchain_path, toolchain_content) {
        messages::error(&format!("Failed to write .cim/toolchain.mk: {}", e));
        return;
    }

    // 2. generated.mk (the single entry point included by the root Makefile)
    let generated_content = generate_cim_generated_mk_content(sdk_config, workspace_path);
    let generated_path = cim_dir.join("generated.mk");
    if let Err(e) = std::fs::write(&generated_path, generated_content) {
        messages::error(&format!("Failed to write .cim/generated.mk: {}", e));
        return;
    }

    messages::success("Generated .cim/ directory (toolchain.mk, generated.mk)");
}

/// Derive a Make variable name from a toolchain destination path.
///
/// `toolchains/aarch64-bm`  →  `TOOLCHAIN_AARCH64_BM`
/// `toolchains/arm-none`    →  `TOOLCHAIN_ARM_NONE`
fn toolchain_var_name(destination: &str) -> String {
    let last = std::path::Path::new(destination)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(destination);
    let sanitized = last.to_uppercase().replace('-', "_").replace('.', "_");
    format!("TOOLCHAIN_{}", sanitized)
}

/// Generate the content of `.cim/toolchain.mk`.
///
/// This file is the single source of truth for all toolchain variables in the
/// workspace. Component makefiles (-include this via generated.mk) instead of
/// each re-discovering paths themselves.
fn generate_toolchain_mk_content(sdk_config: &config::SdkConfig) -> String {
    let mut mk = String::new();

    mk.push_str("# Generated by 'cim makefile' — do not edit. Re-run to update.\n");
    mk.push_str("# Single source of truth for workspace toolchain variables.\n\n");

    // Emit all manifest variables as weak assignments so host env vars override them
    let vars = if let Some(raw_vars) = &sdk_config.variables {
        dsdk_cli::workspace::resolve_variables(raw_vars)
    } else {
        std::collections::HashMap::new()
    };

    if !vars.is_empty() {
        mk.push_str("# Manifest variables\n");
        let mut sorted: Vec<_> = vars.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (key, value) in &sorted {
            mk.push_str(&format!("{} ?= {}\n", key, value));
        }
        mk.push('\n');
    }

    // Emit toolchain path variables derived from toolchain destinations
    if let Some(toolchains) = &sdk_config.toolchains {
        // Deduplicate destinations so multi-platform entries don't repeat the variable
        let mut seen_destinations = std::collections::HashSet::new();
        let mut toolchain_vars: Vec<(String, String)> = Vec::new();

        for tc in toolchains {
            if seen_destinations.insert(tc.destination.clone()) {
                let var_name = toolchain_var_name(&tc.destination);
                toolchain_vars.push((var_name, tc.destination.clone()));
            }
        }

        if !toolchain_vars.is_empty() {
            mk.push_str("# Toolchain path variables (derived from sdk.yml toolchains:)\n");
            for (var_name, dest) in &toolchain_vars {
                mk.push_str(&format!("{} ?= $(WORKSPACE)/{}/bin\n", var_name, dest));
            }
            mk.push('\n');

            // macOS: prepend Homebrew bin so modern bash/bison/flex are found
            mk.push_str("# macOS: prepend Homebrew bin to PATH for modern tooling\n");
            mk.push_str("ifeq ($(shell uname -s),Darwin)\n");
            mk.push_str("export PATH := /opt/homebrew/bin:$(PATH)\n");
            mk.push_str("endif\n\n");

            // Export toolchain paths into PATH for all subsequent commands
            mk.push_str("# Export toolchain bin directories into PATH\n");
            // Build the PATH prefix from all toolchain var names
            let path_prefix: Vec<String> = toolchain_vars
                .iter()
                .map(|(var_name, _)| format!("$({})", var_name))
                .collect();
            mk.push_str(&format!(
                "export PATH := {}:$(PATH)\n\n",
                path_prefix.join(":")
            ));
        }
    }

    // ccache detection — available to all repos
    mk.push_str("# ccache detection (optional, speeds up recompilation)\n");
    mk.push_str("CCACHE := $(shell command -v ccache 2>/dev/null)\n");

    mk
}

/// Generate the content of a per-repo generated stub `.cim/<repo>.mk`.
///
/// Stubs are a mechanical translation of sdk.yml `build:` commands and are
/// only generated for repos that have neither a `.config/cim/cim.mk` in the
/// repo nor a registered overlay in sdk.yml `overlays:`.
///
/// **Do not edit generated stubs.** Put customisations in
/// `<repo>/.config/cim/cim.mk` (repos you own) or in an explicit overlay
/// registered via sdk.yml `overlays:` (third-party repos).

/// Generate the content of `.cim/generated.mk`.
///
/// This is the single file included by the root Makefile (`-include .cim/generated.mk`).
/// It pulls in, in order:
///   1. toolchain.mk   — workspace toolchain variables
///   2. Overlay files  — from sdk.yml `overlays:` (manually maintained, typically in build.git)
///   3. Repo cim.mk    — from `<repo>/.config/cim/cim.mk` (self-describing repos)
///   4. Stubs          — for repos with no cim.mk and no overlay
fn generate_cim_generated_mk_content(
    sdk_config: &config::SdkConfig,
    workspace_path: &std::path::Path,
) -> String {
    let mut mk = String::new();

    mk.push_str("# Auto-generated by 'cim makefile' — do not edit. Re-run to update.\n");
    mk.push_str(
        "# Single include entry point. The root Makefile uses: -include .cim/generated.mk\n\n",
    );

    // 1. Toolchain vars — always first so everything below can use them
    mk.push_str("# Workspace toolchain variables\n");
    mk.push_str("-include $(WORKSPACE)/.cim/toolchain.mk\n\n");

    // Build overlay lookup: repo_name → source path
    let overlay_map: std::collections::HashMap<String, String> = sdk_config
        .overlays
        .as_ref()
        .map(|ovs| {
            ovs.iter()
                .map(|o| (o.for_repo.clone(), o.source.clone()))
                .collect()
        })
        .unwrap_or_default();

    // 2. Overlays (third-party repos — manually maintained, registered in sdk.yml overlays:)
    if !overlay_map.is_empty() {
        mk.push_str("# Overlays: manually maintained build adaptors for third-party repos\n");
        // Output in a stable order (sorted by repo name)
        let mut sorted_overlays: Vec<_> = overlay_map.iter().collect();
        sorted_overlays.sort_by_key(|(k, _)| k.as_str());
        for (repo, source) in &sorted_overlays {
            mk.push_str(&format!(
                "-include $(WORKSPACE)/{}  # overlay for {}\n",
                source, repo
            ));
        }
        mk.push('\n');
    }

    // Per-repo: include .config/cim/cim.mk when present (overlays already handled above)
    let cim_mk_includes: Vec<String> = sdk_config
        .gits
        .iter()
        .filter(|git| !overlay_map.contains_key(&git.name))
        .filter(|git| {
            workspace_path
                .join(&git.name)
                .join(".config/cim/cim.mk")
                .exists()
        })
        .map(|git| format!("-include $(WORKSPACE)/{}/.config/cim/cim.mk\n", git.name))
        .collect();

    if !cim_mk_includes.is_empty() {
        mk.push_str("# Per-repo build adaptors (repo-owned .config/cim/cim.mk)\n");
        for line in &cim_mk_includes {
            mk.push_str(line);
        }
        mk.push('\n');
    }

    mk
}

/// Render a command string for inclusion in a Ninja build rule.
///
/// Ninja uses `$var` and `${var}` as its own variable expansion syntax, and
/// `$(var)` as an alternative. Everything with a single `$` is intercepted by
/// Ninja before the string reaches the shell. To get a literal `$` in the
/// shell command, write `$$` in the Ninja file.
///
/// This function translates Make-style variable references for Ninja:
///
/// - `$(MAKE)` → `${MAKE}`: Ninja expands `${MAKE}` using the `MAKE` Ninja
///   variable, which is defined in the generated ninja file as `make`.
///   The shell receives the literal string `make`, not a variable reference.
///
/// - `$(WORKSPACE)` → `${workspace}`: Similarly, Ninja expands this using the
///   `workspace` Ninja variable (absolute path defined at generation time).
///
/// - Other `$(VAR)` → `$$(VAR)`: Ninja sees `$$` and emits a literal `$`
///   to the shell, which then sees `$(VAR)` and can expand it from its env.
///
/// Also applies `${{ VAR }}` → `$(VAR)` substitution before escaping.
pub(crate) fn render_command_for_ninja(cmd: &str) -> String {
    // First apply manifest variable substitution (${{ VAR }} → $(VAR))
    let make_form = render_command_for_makefile(cmd);
    // $(MAKE) → ${MAKE}: Ninja variable "MAKE = make" defined in ninja file
    let s = make_form.replace("$(MAKE)", "${MAKE}");
    // $(WORKSPACE) → ${workspace}: Ninja variable with absolute path
    let s = s.replace("$(WORKSPACE)", "${workspace}");
    // All remaining $(...) → $$(...) so Ninja passes them to the shell literally
    s.replace("$(", "$$(")
}

// ─── Ninja file generation ────────────────────────────────────────────────────

/// Generate `.cim/build.ninja` and write it to the workspace.
///
/// The Ninja file encodes the same workspace-level dependency graph as the root
/// Makefile but lets Ninja exploit it for parallel builds. Independent repos
/// (those with no `build_depends_on`) start simultaneously; repos that depend
/// on others wait until all their dependencies finish.
///
/// Each repo's *internal* build system is unchanged — Ninja simply calls
/// `make <target>` for each repo in the right order. The toolchain variables
/// (CROSS_COMPILE, PATH, etc.) are available because each `make <target>` call
/// reads the workspace root Makefile which `-include .cim/generated.mk`.
fn generate_ninja_file(workspace_path: &std::path::Path, sdk_config: &config::SdkConfig) {
    let cim_dir = workspace_path.join(".cim");
    let ninja_path = cim_dir.join("build.ninja");
    let content = generate_ninja_content(workspace_path, sdk_config);
    match std::fs::write(&ninja_path, content) {
        Ok(_) => messages::success(&format!(
            "Generated .cim/build.ninja (parallel builds via ninja)"
        )),
        Err(e) => messages::error(&format!("Failed to write .cim/build.ninja: {}", e)),
    }
}

/// Generate the content of `.cim/build.ninja`.
///
/// Ninja syntax primer used here:
///   `rule NAME`   — defines a command pattern with variables
///   `build T: R D` — target T built by rule R, depending on D
///   `build T: phony` — T is always out-of-date (like .PHONY in Make)
///   `$var`        — Ninja variable reference (single $)
///   `$(MAKE)`     — passed through to the shell as-is (Ninja ≠ Make)
pub(crate) fn generate_ninja_content(
    workspace_path: &std::path::Path,
    sdk_config: &config::SdkConfig,
) -> String {
    let workspace_str = workspace_path.to_string_lossy();
    let mut ninja = String::new();

    // Header
    ninja.push_str("# Auto-generated by 'cim makefile --ninja' — do not edit. Re-run to update.\n");
    ninja
        .push_str("# Encodes the workspace repo dependency graph for parallel builds via ninja.\n");
    ninja.push_str("# Each repo's internal build system (e.g. recursive make) is unchanged;\n");
    ninja.push_str("# ninja only controls the order in which repos start building.\n\n");

    // Ninja variables: workspace (absolute path) and MAKE (resolved at generation time)
    // Using Ninja variables avoids any shell escaping issues for these well-known values.
    ninja.push_str(&format!("workspace = {}\n", workspace_str));
    // MAKE: use the actual make binary path if available, fall back to "make"
    let make_bin = std::env::var("MAKE").unwrap_or_else(|_| "make".to_string());
    ninja.push_str(&format!("MAKE = {}\n", make_bin));

    // Manifest variables (same as root Makefile)
    let vars = if let Some(raw_vars) = &sdk_config.variables {
        dsdk_cli::workspace::resolve_variables(raw_vars)
    } else {
        std::collections::HashMap::new()
    };
    if !vars.is_empty() {
        let mut sorted: Vec<_> = vars.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (key, value) in &sorted {
            // Ninja variables are lowercase by convention; emit both for shell commands
            ninja.push_str(&format!("{} = {}\n", key.to_lowercase(), value));
        }
    }
    ninja.push('\n');

    // Rule: run a make target from the workspace root.
    // $(MAKE) in the command string is passed through to the shell, not interpreted by Ninja.
    // The shell expands $(MAKE) to the make binary (inherited from environment).
    ninja.push_str("rule make_target\n");
    ninja.push_str("  command = cd $workspace && $cmd\n");
    ninja.push_str("  description = [$name] $cmd\n\n");

    // ── Per-repo build targets ────────────────────────────────────────────────
    if !sdk_config.gits.is_empty() {
        ninja.push_str("# Repository build targets\n");
        for git in &sdk_config.gits {
            let deps = git
                .build_depends_on
                .as_ref()
                .map(|d| d.join(" "))
                .unwrap_or_default();

            let target_line = if deps.is_empty() {
                format!("build {}: make_target\n", git.name)
            } else {
                format!("build {}: make_target {}\n", git.name, deps)
            };
            ninja.push_str(&target_line);
            ninja.push_str(&format!("  name = {}\n", git.name));

            // Join multiple commands with ' && ' (run sequentially within one Ninja rule)
            let cmd = if let Some(build_cmds) = &git.build {
                build_cmds
                    .iter()
                    .map(|c| render_command_for_ninja(c))
                    .collect::<Vec<_>>()
                    .join(" && ")
            } else {
                // Default: call the git target by name via make
                format!("$$(MAKE) {}", git.name)
            };
            ninja.push_str(&format!("  cmd = {}\n\n", cmd));
        }
    }

    // ── SDK-level targets ─────────────────────────────────────────────────────
    ninja.push_str("# SDK-level targets\n");

    // sdk-build: runs after all repo deps are built; commands from sdk.yml build:
    if let Some(build_target) = &sdk_config.build {
        let deps = build_target
            .depends_on()
            .map(|d| d.join(" "))
            .unwrap_or_default();
        let target_line = if deps.is_empty() {
            "build sdk-build: make_target\n".to_string()
        } else {
            format!("build sdk-build: make_target {}\n", deps)
        };
        ninja.push_str(&target_line);
        ninja.push_str("  name = sdk-build\n");
        let cmd = build_target
            .commands()
            .iter()
            .map(|c| render_command_for_ninja(c))
            .collect::<Vec<_>>()
            .join(" && ");
        ninja.push_str(&format!("  cmd = {}\n\n", cmd));
    } else {
        ninja.push_str("build sdk-build: phony\n\n");
    }

    // sdk-clean
    if let Some(clean_target) = &sdk_config.clean {
        let cmd = clean_target
            .commands()
            .iter()
            .map(|c| render_command_for_ninja(c))
            .collect::<Vec<_>>()
            .join(" && ");
        ninja.push_str("build sdk-clean: make_target\n");
        ninja.push_str("  name = sdk-clean\n");
        ninja.push_str(&format!("  cmd = {}\n\n", cmd));
    }

    // sdk-test
    if let Some(test_target) = &sdk_config.test {
        let cmd = test_target
            .commands()
            .iter()
            .map(|c| render_command_for_ninja(c))
            .collect::<Vec<_>>()
            .join(" && ");
        ninja.push_str("build sdk-test: make_target sdk-build\n");
        ninja.push_str("  name = sdk-test\n");
        ninja.push_str(&format!("  cmd = {}\n\n", cmd));
    }

    // sdk-flash
    if let Some(flash_target) = &sdk_config.flash {
        let cmd = flash_target
            .commands()
            .iter()
            .map(|c| render_command_for_ninja(c))
            .collect::<Vec<_>>()
            .join(" && ");
        ninja.push_str("build sdk-flash: make_target\n");
        ninja.push_str("  name = sdk-flash\n");
        ninja.push_str(&format!("  cmd = {}\n\n", cmd));
    }

    // Default target
    ninja.push_str("default sdk-build\n");

    ninja
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_makefile_content_empty() {
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);
        assert!(
            makefile.starts_with(&format!("{}\n\n", WORKSPACE_VARIABLE)),
            "Expected WORKSPACE variable first in Makefile, got:\n{}",
            makefile
        );
        assert!(makefile.contains(".PHONY: all"));
        assert!(makefile.contains("all:"));
    }

    #[test]
    fn test_generate_makefile_content_single_repo() {
        let git_config = config::GitConfig {
            name: "test-repo".to_string(),
            url: "https://github.com/test/repo.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: Some(vec!["make".to_string(), "make install".to_string()]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![git_config],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);
        assert!(makefile.contains(".PHONY: all"));
        assert!(makefile.contains("all: sdk-build"));
        assert!(makefile.contains("test-repo:"));
        assert!(makefile.contains("\tmake"));
        assert!(makefile.contains("\tmake install"));
    }

    #[test]
    fn test_generate_makefile_content_with_dependencies() {
        let git1 = config::GitConfig {
            name: "base-repo".to_string(),
            url: "https://github.com/test/base.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: Some(vec!["@echo Building base".to_string()]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let git2 = config::GitConfig {
            name: "dep-repo".to_string(),
            url: "https://github.com/test/dep.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: Some(vec!["base-repo".to_string()]),
            git_depends_on: None,
            build: Some(vec!["# This is a comment".to_string(), "make".to_string()]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![git1, git2],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);
        assert!(makefile.contains("all: sdk-build"));
        assert!(makefile.contains("base-repo:"));
        assert!(makefile.contains("dep-repo: base-repo"));
        assert!(makefile.contains("\t@echo Building base"));
        assert!(makefile.contains("\t#This is a comment"));
        assert!(makefile.contains("\tmake"));
    }

    #[test]
    fn test_add_makefile_target_no_deps_no_build() {
        let mut makefile = String::new();
        let git_config = config::GitConfig {
            name: "simple-repo".to_string(),
            url: "https://github.com/test/simple.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: None,
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        add_makefile_target(&mut makefile, &git_config);

        assert!(makefile.contains("simple-repo:"));
        assert!(makefile.contains("\t@echo Building simple-repo"));
    }

    #[test]
    fn test_add_makefile_target_with_comments() {
        let mut makefile = String::new();
        let git_config = config::GitConfig {
            name: "commented-repo".to_string(),
            url: "https://github.com/test/commented.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: Some(vec![
                "# Configure the build".to_string(),
                "./configure".to_string(),
                "#Another comment".to_string(),
                "make".to_string(),
            ]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        add_makefile_target(&mut makefile, &git_config);

        assert!(makefile.contains("commented-repo:"));
        assert!(makefile.contains("\t#Configure the build"));
        assert!(makefile.contains("\t./configure"));
        assert!(makefile.contains("\t#Another comment"));
        assert!(makefile.contains("\tmake"));
    }

    #[test]
    fn test_makefile_generation_edge_cases() {
        // Test with repository that has empty build commands
        let git_config = config::GitConfig {
            name: "empty-build".to_string(),
            url: "https://github.com/test/empty.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: Some(vec![]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![git_config],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);
        assert!(makefile.contains("empty-build:"));

        // Test with multiple dependencies
        let git_with_many_deps = config::GitConfig {
            name: "many-deps".to_string(),
            url: "https://github.com/test/many.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: Some(vec![
                "dep1".to_string(),
                "dep2".to_string(),
                "dep3".to_string(),
            ]),
            git_depends_on: None,
            build: Some(vec!["echo hello".to_string()]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let mut makefile = String::new();
        add_makefile_target(&mut makefile, &git_with_many_deps);
        assert!(makefile.contains("many-deps: dep1 dep2 dep3"));
    }

    #[test]
    fn test_envsetup_target_generation() {
        // Test with envsetup commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: Some(config::SdkTarget::Commands(vec![
                "ln -sf qemu_v8.mk build/Makefile".to_string(),
                "cd build && make -j3 toolchains".to_string(),
            ])),
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Check that .PHONY includes sdk-envsetup
        assert!(makefile.contains(".PHONY: all sdk-envsetup"));

        // Check that sdk-envsetup target exists
        assert!(makefile.contains("sdk-envsetup:"));

        // Check that commands are properly formatted with tabs
        assert!(makefile.contains("\tln -sf qemu_v8.mk build/Makefile"));
        assert!(makefile.contains("\tcd build && make -j3 toolchains"));
    }

    #[test]
    fn test_envsetup_target_with_comments_and_echo() {
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: Some(config::SdkTarget::Commands(vec![
                "# Setup toolchain".to_string(),
                "@echo Setting up environment".to_string(),
                "mkdir -p build".to_string(),
                "#Another comment".to_string(),
                "export CROSS_COMPILE=aarch64-linux-gnu-".to_string(),
            ])),
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Check that comments are preserved
        assert!(makefile.contains("#Setup toolchain"));
        assert!(makefile.contains("#Another comment"));

        // Check that @ echo commands are converted properly
        assert!(makefile.contains("\t@echo Setting up environment"));

        // Check that regular commands are included
        assert!(makefile.contains("\tmkdir -p build"));
        assert!(makefile.contains("\texport CROSS_COMPILE=aarch64-linux-gnu-"));
    }

    #[test]
    fn test_envsetup_target_empty_commands() {
        // Test with empty envsetup commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Should not include sdk-envsetup in PHONY or create target
        assert!(makefile.contains(".PHONY: all"));
        assert!(!makefile.contains("sdk-envsetup"));
    }

    #[test]
    fn test_envsetup_target_none() {
        // Test with no envsetup commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Should not include sdk-envsetup
        assert!(makefile.contains(".PHONY: all"));
        assert!(!makefile.contains("sdk-envsetup"));
    }

    #[test]
    fn test_add_envsetup_target_function() {
        let mut makefile = String::new();
        let target = config::SdkTarget::Commands(vec![
            "# Initial setup".to_string(),
            "mkdir -p logs".to_string(),
            "@echo Starting setup".to_string(),
            "chmod +x scripts/setup.sh".to_string(),
        ]);

        add_envsetup_target(&mut makefile, &target);

        assert!(makefile.contains("sdk-envsetup:"));
        assert!(makefile.contains("\t#Initial setup"));
        assert!(makefile.contains("\tmkdir -p logs"));
        assert!(makefile.contains("\t@echo Starting setup"));
        assert!(makefile.contains("\tchmod +x scripts/setup.sh"));

        // Should end with blank line
        assert!(makefile.ends_with("\n\n"));
    }

    #[test]
    fn test_test_target_generation() {
        // Test with test commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: Some(config::SdkTarget::Commands(vec![
                "cargo test --release".to_string(),
                "python run_integration_tests.py".to_string(),
            ])),
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Check that .PHONY includes sdk-test
        assert!(makefile.contains(".PHONY: all sdk-test"));

        // Check that all depends on both sdk-build and sdk-test
        assert!(makefile.contains("all: sdk-build sdk-test"));

        // Check that sdk-test target exists
        assert!(makefile.contains("sdk-test:"));

        // Check that commands are properly formatted with tabs
        assert!(makefile.contains("\tcargo test --release"));
        assert!(makefile.contains("\tpython run_integration_tests.py"));
    }

    #[test]
    fn test_test_target_with_comments_and_echo() {
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: Some(config::SdkTarget::Commands(vec![
                "# Run unit tests".to_string(),
                "@echo Running test suite".to_string(),
                "cargo test".to_string(),
                "#Run integration tests".to_string(),
                "pytest integration/".to_string(),
            ])),
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Check that comments are preserved
        assert!(makefile.contains("#Run unit tests"));
        assert!(makefile.contains("#Run integration tests"));

        // Check that @ echo commands are converted properly
        assert!(makefile.contains("\t@echo Running test suite"));

        // Check that regular commands are included
        assert!(makefile.contains("\tcargo test"));
        assert!(makefile.contains("\tpytest integration/"));
    }

    #[test]
    fn test_test_target_empty_commands() {
        // Test with empty test commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Should not include sdk-test in PHONY or create target
        assert!(makefile.contains(".PHONY: all"));
        assert!(!makefile.contains("sdk-test"));
    }

    #[test]
    fn test_test_target_none() {
        // Test with no test commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Should not include sdk-test
        assert!(makefile.contains(".PHONY: all"));
        assert!(!makefile.contains("sdk-test"));
    }

    #[test]
    fn test_add_test_target_function() {
        let mut makefile = String::new();
        let target = config::SdkTarget::Commands(vec![
            "# Run comprehensive tests".to_string(),
            "mkdir -p test-results".to_string(),
            "@echo Starting test execution".to_string(),
            "cargo test --verbose".to_string(),
        ]);

        add_test_target(&mut makefile, &target);

        assert!(makefile.contains("sdk-test:"));
        assert!(makefile.contains("\t#Run comprehensive tests"));
        assert!(makefile.contains("\tmkdir -p test-results"));
        assert!(makefile.contains("\t@echo Starting test execution"));
        assert!(makefile.contains("\tcargo test --verbose"));

        // Should end with blank line
        assert!(makefile.ends_with("\n\n"));
    }

    #[test]
    fn test_combined_envsetup_and_test_targets() {
        // Test with both envsetup and test commands
        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: Some(config::SdkTarget::Commands(vec![
                "make configure".to_string()
            ])),
            test: Some(config::SdkTarget::Commands(vec!["make test".to_string()])),
            clean: None,
            build: None,
            flash: None,
            variables: None,
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Check that .PHONY includes both targets
        assert!(makefile.contains(".PHONY: all sdk-envsetup sdk-test"));

        // Check that all depends on sdk-build and sdk-test
        assert!(makefile.contains("all: sdk-build sdk-test"));

        // Check that both targets exist
        assert!(makefile.contains("sdk-envsetup:"));
        assert!(makefile.contains("sdk-test:"));

        // Check that commands are properly formatted
        assert!(makefile.contains("\tmake configure"));
        assert!(makefile.contains("\tmake test"));
    }

    #[test]
    fn test_render_command_for_makefile_substitution() {
        // ${{ VAR }} becomes $(VAR)
        assert_eq!(
            render_command_for_makefile("DOCKER_DEFAULT_PLATFORM=${{ PLATFORM }} ./run.sh"),
            "DOCKER_DEFAULT_PLATFORM=$(PLATFORM) ./run.sh"
        );

        // Multiple vars in one command
        assert_eq!(
            render_command_for_makefile("${{ CMD }} --platform ${{ PLATFORM }}"),
            "$(CMD) --platform $(PLATFORM)"
        );

        // Existing Make variable references are preserved
        assert_eq!(
            render_command_for_makefile("$(MAKE) -C build all $(MAKEFLAGS)"),
            "$(MAKE) -C build all $(MAKEFLAGS)"
        );

        // Command without any variables is unchanged
        assert_eq!(
            render_command_for_makefile("cd repo && ./build.sh"),
            "cd repo && ./build.sh"
        );

        // Unclosed ${{ is left unchanged
        assert_eq!(
            render_command_for_makefile("echo ${{ NOCLOSE"),
            "echo ${{ NOCLOSE"
        );
    }

    #[test]
    fn test_generate_makefile_with_variables() {
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "DOCKER_DEFAULT_PLATFORM".to_string(),
            "linux/amd64".to_string(),
        );

        let config = config::SdkConfig {
            toolchains: None,
            install: Some(vec![config::InstallConfig {
                name: "devcontainer".to_string(),
                depends_on: None,
                sentinel: Some("opt/.devcontainer-installed".to_string()),
                commands: Some(vec![
                    "cd repo && DOCKER_DEFAULT_PLATFORM=${{ DOCKER_DEFAULT_PLATFORM }} ./run.sh --new".to_string(),
                ]),
            }]),
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: Some(vars),
        };

        let makefile = generate_makefile_content(&config, false, false);

        let workspace_index = makefile
            .find(WORKSPACE_VARIABLE)
            .expect("WORKSPACE variable should be emitted");
        let manifest_var_index = makefile
            .find("DOCKER_DEFAULT_PLATFORM ?= linux/amd64")
            .expect("manifest variable should be emitted");

        assert!(
            workspace_index < manifest_var_index,
            "WORKSPACE should be emitted before manifest variables, got:\n{}",
            makefile
        );

        // ?= assignment emitted for the manifest variable
        assert!(
            makefile.contains("DOCKER_DEFAULT_PLATFORM ?= linux/amd64"),
            "Expected ?= assignment in Makefile, got:\n{}",
            makefile
        );

        // ${{ VAR }} in install command becomes $(VAR)
        assert!(
            makefile.contains("$(DOCKER_DEFAULT_PLATFORM)"),
            "Expected Make variable reference in command, got:\n{}",
            makefile
        );

        // The raw ${{ }} syntax should NOT appear in the output
        assert!(
            !makefile.contains("${{"),
            "Raw manifest variable syntax should not appear in Makefile"
        );
    }

    #[test]
    fn test_variables_before_includes() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("PLATFORMS_ROOT".to_string(), "platforms".to_string());

        let config = config::SdkConfig {
            toolchains: None,
            install: None,
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![],
            copy_files: None,
            makefile_include: Some(vec!["include build/extra.mk".to_string()]),
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: Some(vars),
        };

        let makefile = generate_makefile_content(&config, false, false);

        let vars_pos = makefile.find("?=").expect("variables block missing");
        let include_pos = makefile.find("-include").expect("include block missing");
        assert!(
            vars_pos < include_pos,
            "Expected variables before -include, got:\n{}",
            makefile
        );
    }

    #[test]
    fn test_generate_makefile_with_dividers() {
        let git_config = config::GitConfig {
            name: "test-repo".to_string(),
            url: "https://github.com/test/repo.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: Some(vec!["make".to_string()]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let mut vars = std::collections::HashMap::new();
        vars.insert("MY_VAR".to_string(), "value".to_string());

        let config = config::SdkConfig {
            toolchains: None,
            install: Some(vec![config::InstallConfig {
                name: "my-tool".to_string(),
                depends_on: None,
                sentinel: Some("opt/.my-tool-installed".to_string()),
                commands: Some(vec!["echo install".to_string()]),
            }]),
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![git_config],
            copy_files: None,
            makefile_include: Some(vec!["include extra.mk".to_string()]),
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: Some(vars),
        };

        let makefile = generate_makefile_content(&config, true, false);

        // Verify divider banners are present
        assert!(
            makefile.contains("# Workspace and manifest variables\n"),
            "Expected 'Workspace and manifest variables' divider, got:\n{}",
            makefile
        );
        assert!(
            makefile.contains("# Makefile includes\n"),
            "Expected 'Makefile includes' divider, got:\n{}",
            makefile
        );
        assert!(
            makefile.contains("# High-level SDK targets\n"),
            "Expected 'High-level SDK targets' divider, got:\n{}",
            makefile
        );
        assert!(
            makefile.contains("# Installation targets\n"),
            "Expected 'Installation targets' divider, got:\n{}",
            makefile
        );
        assert!(
            makefile.contains("# Git repository targets\n"),
            "Expected 'Git repository targets' divider, got:\n{}",
            makefile
        );

        // Verify the divider format uses 80-char # lines
        assert!(
            makefile.contains("################################################################################\n# Workspace and manifest variables\n################################################################################"),
            "Expected full divider banner format"
        );
    }

    #[test]
    fn test_generate_makefile_without_dividers() {
        let git_config = config::GitConfig {
            name: "test-repo".to_string(),
            url: "https://github.com/test/repo.git".to_string(),
            commit: "main".to_string(),
            build_depends_on: None,
            git_depends_on: None,
            build: Some(vec!["make".to_string()]),
            documentation_dir: None,
            optional: false,
            build_entry: None,
            toolchain_vars: None,
        };

        let mut vars = std::collections::HashMap::new();
        vars.insert("MY_VAR".to_string(), "value".to_string());

        let config = config::SdkConfig {
            toolchains: None,
            install: Some(vec![config::InstallConfig {
                name: "my-tool".to_string(),
                depends_on: None,
                sentinel: Some("opt/.my-tool-installed".to_string()),
                commands: Some(vec!["echo install".to_string()]),
            }]),
            mirror: PathBuf::from("/tmp/mirror"),
            gits: vec![git_config],
            copy_files: None,
            makefile_include: Some(vec!["include extra.mk".to_string()]),
            overlays: None,
            envsetup: None,
            test: None,
            clean: None,
            build: None,
            flash: None,
            variables: Some(vars),
        };

        let makefile = generate_makefile_content(&config, false, false);

        // Verify no divider banners are present
        assert!(
            !makefile.contains("########"),
            "Expected no divider banners when dividers=false, got:\n{}",
            makefile
        );
    }

    #[test]
    fn test_makefile_divider_format() {
        let divider = makefile_divider("Test Section");
        assert_eq!(
            divider,
            "################################################################################\n\
             # Test Section\n\
             ################################################################################\n"
        );
    }

    // ── Ninja generation tests ────────────────────────────────────────────────

    fn make_ninja_sdk_config(workspace: &std::path::Path) -> config::SdkConfig {
        let mut vars = std::collections::HashMap::new();
        vars.insert("CROSS_COMPILE".to_string(), "aarch64-none-elf-".to_string());
        config::SdkConfig {
            mirror: workspace.join("mirror"),
            gits: vec![
                config::GitConfig {
                    name: "u-boot".to_string(),
                    url: "https://github.com/u-boot/u-boot.git".to_string(),
                    commit: "master".to_string(),
                    build_depends_on: None,
                    git_depends_on: None,
                    build: Some(vec!["$(MAKE) uboot-build".to_string()]),
                    documentation_dir: None,
                    optional: false,
                    build_entry: None,
                    toolchain_vars: None,
                },
                config::GitConfig {
                    name: "hello-world".to_string(),
                    url: "https://github.com/example/hello-world.git".to_string(),
                    commit: "main".to_string(),
                    build_depends_on: None,
                    git_depends_on: None,
                    build: Some(vec!["$(MAKE) hello-world-build".to_string()]),
                    documentation_dir: None,
                    optional: false,
                    build_entry: None,
                    toolchain_vars: None,
                },
                config::GitConfig {
                    name: "build".to_string(),
                    url: "https://github.com/example/build.git".to_string(),
                    commit: "main".to_string(),
                    build_depends_on: Some(vec!["u-boot".to_string(), "hello-world".to_string()]),
                    git_depends_on: None,
                    build: Some(vec!["$(MAKE) hello-world-build".to_string()]),
                    documentation_dir: None,
                    optional: false,
                    build_entry: None,
                    toolchain_vars: None,
                },
            ],
            toolchains: None,
            copy_files: None,
            install: None,
            makefile_include: None,
            overlays: None,
            envsetup: None,
            test: Some(config::SdkTarget::Commands(vec![
                "$(MAKE) -f build/Makefile qemu WORKSPACE=$(WORKSPACE)".to_string(),
            ])),
            clean: Some(config::SdkTarget::Commands(vec![
                "$(MAKE) uboot-clean".to_string()
            ])),
            build: Some(config::SdkTarget::CommandsWithDeps {
                commands: vec![
                    "@echo Building".to_string(),
                    "$(MAKE) -C build qemu-hello WORKSPACE=$(WORKSPACE)".to_string(),
                ],
                depends_on: Some(vec!["u-boot".to_string(), "hello-world".to_string()]),
            }),
            flash: None,
            variables: Some(vars),
        }
    }

    #[test]
    fn test_generate_ninja_content_basic() {
        let workspace = PathBuf::from("/workspace/test");
        let sdk_config = make_ninja_sdk_config(&workspace);
        let content = generate_ninja_content(&workspace, &sdk_config);

        // Header
        assert!(content.contains("Auto-generated by 'cim makefile --ninja'"));
        // Workspace variable
        assert!(content.contains("workspace = /workspace/test"));
        // Manifest variable (lowercase)
        assert!(content.contains("cross_compile = aarch64-none-elf-"));
        // Rule definition
        assert!(content.contains("rule make_target"));
        assert!(content.contains("command = cd $workspace && $cmd"));
        // Repo targets present
        assert!(content.contains("build u-boot: make_target"));
        assert!(content.contains("build hello-world: make_target"));
        // Default target
        assert!(content.contains("default sdk-build"));
    }

    #[test]
    fn test_generate_ninja_deps_order() {
        let workspace = PathBuf::from("/workspace/test");
        let sdk_config = make_ninja_sdk_config(&workspace);
        let content = generate_ninja_content(&workspace, &sdk_config);

        // u-boot and hello-world have no deps — no trailing names
        assert!(content.contains("build u-boot: make_target\n"));
        assert!(content.contains("build hello-world: make_target\n"));
        // build depends on u-boot and hello-world
        assert!(content.contains("build build: make_target u-boot hello-world\n"));
        // sdk-build depends on u-boot and hello-world (from SdkTarget depends_on)
        assert!(content.contains("build sdk-build: make_target u-boot hello-world\n"));
    }

    #[test]
    fn test_ninja_command_join() {
        let workspace = PathBuf::from("/workspace/test");
        let mut sdk_config = make_ninja_sdk_config(&workspace);
        // Give u-boot two build commands
        sdk_config.gits[0].build = Some(vec![
            "$(MAKE) defconfig".to_string(),
            "$(MAKE) all".to_string(),
        ]);
        let content = generate_ninja_content(&workspace, &sdk_config);
        // $(MAKE) → ${MAKE} (Ninja variable), commands joined with ' && '
        assert!(content.contains("${MAKE} defconfig && ${MAKE} all"));
    }

    #[test]
    fn test_makefile_ninja_autodetect() {
        let workspace = PathBuf::from("/tmp/test");
        let sdk_config = make_ninja_sdk_config(&workspace);
        let makefile = generate_makefile_content(&sdk_config, false, true);

        // Should have NINJA detection variables
        assert!(makefile.contains("NINJA := $(shell command -v ninja 2>/dev/null)"));
        assert!(makefile.contains("NINJA_BUILD_FILE := $(WORKSPACE)/.cim/build.ninja"));
        // sdk-build should contain the ifneq auto-detect block
        assert!(makefile.contains("ifneq ($(NINJA),)"));
        assert!(makefile.contains("ifneq ($(wildcard $(NINJA_BUILD_FILE)),)"));
        assert!(makefile.contains("$(NINJA) -f $(NINJA_BUILD_FILE) sdk-build"));
    }

    #[test]
    fn test_makefile_no_ninja_autodetect_without_flag() {
        let workspace = PathBuf::from("/tmp/test");
        let sdk_config = make_ninja_sdk_config(&workspace);
        let makefile = generate_makefile_content(&sdk_config, false, false);

        // Without --ninja, no detection variables or conditional blocks
        assert!(!makefile.contains("NINJA :="));
        assert!(!makefile.contains("NINJA_BUILD_FILE"));
        assert!(!makefile.contains("ifneq ($(NINJA),)"));
    }
}
