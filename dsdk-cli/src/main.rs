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

mod cli;
mod init_cmd;
mod install_cmd;
mod makefile;
mod release_cmd;
mod update_cmd;
mod utils_cmd;
mod version;

use clap::{CommandFactory, Parser};
use cli::{Cli, Commands, FragmentCommand};
use dsdk_cli::messages;
use init_cmd::{
    handle_add_command, handle_docs_command, handle_foreach_command, handle_init_command,
    InitConfig,
};
use install_cmd::handle_install_command;
use makefile::handle_makefile_command;
use release_cmd::handle_release_command;
use update_cmd::{
    handle_config_command, handle_docker_command, handle_list_targets_command,
    handle_update_command, ConfigOptions,
};
use utils_cmd::handle_utils_command;
use version::{print_update_notice, print_version_info, spawn_version_check};

fn handle_fragment_command(command: &FragmentCommand) {
    use dsdk_cli::config::load_fragment;
    use dsdk_cli::fragment::discover_fragments_in_dir;
    use dsdk_cli::workspace::get_current_workspace;

    let workspace_path = match get_current_workspace() {
        Ok(path) => path,
        Err(e) => {
            messages::error(&e);
            return;
        }
    };

    let fragments_dir = workspace_path.join(".cim").join("fragments");

    match command {
        FragmentCommand::List => {
            let fragments = match discover_fragments_in_dir(&fragments_dir) {
                Ok(f) => f,
                Err(e) => {
                    messages::error(&format!("Failed to read fragments directory: {}", e));
                    return;
                }
            };

            if fragments.is_empty() {
                messages::info("No workspace fragments found");
                messages::info(&format!("Add fragments to: {}", fragments_dir.display()));
                return;
            }

            messages::status(&format!("Workspace fragments ({}):", fragments.len()));
            for path in &fragments {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                if let Ok(frag) = load_fragment(path) {
                    let desc = frag
                        .fragment
                        .as_ref()
                        .and_then(|m| m.description.as_deref())
                        .unwrap_or("");
                    let frag_name = frag
                        .fragment
                        .as_ref()
                        .and_then(|m| m.name.as_deref())
                        .unwrap_or(&filename);
                    if desc.is_empty() {
                        messages::info(&format!("  {}: {}", filename, frag_name));
                    } else {
                        messages::info(&format!("  {}: {} - {}", filename, frag_name, desc));
                    }
                } else {
                    messages::info(&format!("  {} (parse error)", filename));
                }
            }
        }
        FragmentCommand::Show { name } => {
            // Try as direct path first, then look in fragments dir
            let path = if std::path::Path::new(name).exists() {
                std::path::PathBuf::from(name)
            } else {
                let candidate = fragments_dir.join(format!("{}.fragment.yml", name));
                if candidate.exists() {
                    candidate
                } else {
                    // Try exact name as filename
                    let exact = fragments_dir.join(name);
                    if exact.exists() {
                        exact
                    } else {
                        messages::error(&format!("Fragment not found: {}", name));
                        return;
                    }
                }
            };

            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    messages::status(&format!("Fragment: {}", path.display()));
                    println!("{}", content);
                }
                Err(e) => {
                    messages::error(&format!("Failed to read fragment: {}", e));
                }
            }
        }
        FragmentCommand::Add { paths, force } => {
            // Create fragments directory if needed
            if let Err(e) = std::fs::create_dir_all(&fragments_dir) {
                messages::error(&format!("Failed to create fragments directory: {}", e));
                return;
            }

            for path in paths {
                if !path.exists() {
                    messages::error(&format!("Fragment file not found: {}", path.display()));
                    continue;
                }

                // Validate the fragment parses correctly
                if let Err(e) = load_fragment(path) {
                    messages::error(&format!("Invalid fragment file: {}", e));
                    continue;
                }

                let filename = path.file_name().unwrap_or_default();
                let dest = fragments_dir.join(filename);
                if dest.exists() && !force {
                    messages::error(&format!("Fragment already exists: {}", dest.display()));
                    continue;
                }

                match std::fs::copy(path, &dest) {
                    Ok(_) => {
                        messages::success(&format!(
                            "Added fragment: {}",
                            filename.to_string_lossy()
                        ));
                    }
                    Err(e) => {
                        messages::error(&format!("Failed to copy fragment: {}", e));
                    }
                }
            }
        }
        FragmentCommand::Remove {
            names,
            all,
            interactive,
        } => {
            if *all {
                match dsdk_cli::fragment::discover_fragments_in_dir(&fragments_dir) {
                    Ok(paths) if paths.is_empty() => {
                        messages::info("No fragments to remove");
                    }
                    Ok(paths) => {
                        for path in &paths {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            match std::fs::remove_file(path) {
                                Ok(()) => {
                                    messages::success(&format!("Removed fragment: {}", name));
                                }
                                Err(e) => {
                                    messages::error(&format!("Failed to remove {}: {}", name, e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        messages::error(&format!("Failed to read fragments directory: {}", e));
                    }
                }
            } else if *interactive {
                let paths = match dsdk_cli::fragment::discover_fragments_in_dir(&fragments_dir) {
                    Ok(p) => p,
                    Err(e) => {
                        messages::error(&format!("Failed to read fragments directory: {}", e));
                        return;
                    }
                };
                if paths.is_empty() {
                    messages::info("No fragments to remove");
                    return;
                }

                println!(
                    "Select fragment(s) to remove (comma-separated numbers, or 'q' to cancel):"
                );
                for (i, path) in paths.iter().enumerate() {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    println!("  {}) {}", i + 1, filename);
                }
                print!("> ");
                use std::io::Write;
                std::io::stdout().flush().unwrap_or_default();

                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_err() {
                    messages::error("Failed to read input");
                    return;
                }
                let input = input.trim();
                if input.eq_ignore_ascii_case("q") || input.is_empty() {
                    messages::info("Cancelled");
                    return;
                }

                let indices: Vec<usize> = input
                    .split(',')
                    .filter_map(|s| s.trim().parse::<usize>().ok())
                    .filter(|&n| n >= 1 && n <= paths.len())
                    .collect();

                if indices.is_empty() {
                    messages::error("No valid selection");
                    return;
                }

                for idx in indices {
                    let path = &paths[idx - 1];
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    match std::fs::remove_file(path) {
                        Ok(()) => {
                            messages::success(&format!("Removed fragment: {}", name));
                        }
                        Err(e) => {
                            messages::error(&format!("Failed to remove {}: {}", name, e));
                        }
                    }
                }
            } else if names.is_empty() {
                messages::error(
                    "No fragment names specified. Use --all, --interactive, or provide names.",
                );
            } else {
                for name in names {
                    let path = {
                        let exact = fragments_dir.join(name);
                        if exact.exists() {
                            exact
                        } else {
                            messages::error(&format!("Fragment not found: {}", name));
                            continue;
                        }
                    };

                    match std::fs::remove_file(&path) {
                        Ok(()) => {
                            messages::success(&format!("Removed fragment: {}", name));
                        }
                        Err(e) => {
                            messages::error(&format!("Failed to remove fragment: {}", e));
                        }
                    }
                }
            }
        }
    }
}

fn handle_merge_command(
    targets: &[String],
    output: &std::path::Path,
    source: Option<&str>,
    mirror_override: Option<&std::path::Path>,
    fragments: &[std::path::PathBuf],
    dry_run: bool,
) {
    use dsdk_cli::config::{
        load_config, load_fragment, load_os_dependencies, load_python_dependencies, SdkConfig,
    };
    use dsdk_cli::fragment::{apply_fragment, validate_merged_config};
    use dsdk_cli::merge::{
        format_merged_yaml, format_os_dependencies_yaml, format_python_dependencies_yaml,
        merge_os_dependencies, merge_python_dependencies, merge_targets,
    };
    use dsdk_cli::workspace::get_default_source;

    if targets.len() < 2 {
        messages::error("At least two targets are required for merge");
        std::process::exit(1);
    }

    // Resolve the manifests source directory
    let source_path = source
        .map(|s| s.to_string())
        .unwrap_or_else(get_default_source);
    let config_root = std::path::Path::new(&source_path);

    if !config_root.exists() {
        messages::error(&format!(
            "Manifest source not found: {}",
            config_root.display()
        ));
        std::process::exit(1);
    }

    messages::status(&format!(
        "Merging {} targets: {}",
        targets.len(),
        targets.join(", ")
    ));

    // Load all target configurations and track their directories
    let mut configs: Vec<(String, SdkConfig, std::path::PathBuf)> = Vec::new();
    for target_name in targets {
        // Support both target names and direct paths to target directories
        let config_path = {
            let as_path = std::path::Path::new(target_name);
            let sdk_yml = as_path.join("sdk.yml");
            if sdk_yml.exists() {
                sdk_yml
            } else if as_path.exists() && as_path.is_file() {
                as_path.to_path_buf()
            } else {
                match init_cmd::resolve_target_config(target_name, config_root) {
                    Ok(p) => p,
                    Err(e) => {
                        messages::error(&format!(
                            "Failed to resolve target '{}': {}",
                            target_name, e
                        ));
                        std::process::exit(1);
                    }
                }
            }
        };

        let target_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        let display_name = target_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target_name.clone());

        let config = match load_config(&config_path) {
            Ok(c) => c,
            Err(e) => {
                messages::error(&format!("Failed to load target '{}': {}", display_name, e));
                std::process::exit(1);
            }
        };

        messages::info(&format!(
            "  Loaded '{}' ({} gits)",
            display_name,
            config.gits.len()
        ));
        configs.push((display_name, config, target_dir));
    }

    // Build reference pairs for merge
    let target_refs: Vec<(&str, &SdkConfig)> = configs
        .iter()
        .map(|(name, cfg, _)| (name.as_str(), cfg))
        .collect();

    let mut result = merge_targets(&target_refs, mirror_override);

    // Apply fragments to the merged result, tracking which keys are resolved
    let mut resolved_keys: std::collections::HashSet<(String, String)> = Default::default();
    for frag_path in fragments {
        if !frag_path.exists() {
            messages::error(&format!("Fragment file not found: {}", frag_path.display()));
            std::process::exit(1);
        }
        match load_fragment(frag_path) {
            Ok(fragment) => {
                // Collect keys that this fragment defines or removes — these
                // resolve any corresponding merge conflict.
                if let Some(ref gits) = fragment.gits {
                    for g in gits {
                        resolved_keys.insert(("gits".to_string(), g.name.clone()));
                    }
                }
                if let Some(ref remove) = fragment.remove_gits {
                    for name in remove {
                        resolved_keys.insert(("gits".to_string(), name.clone()));
                    }
                }
                if let Some(ref tcs) = fragment.toolchains {
                    for tc in tcs {
                        if let Some(name) = tc.get_name() {
                            resolved_keys.insert(("toolchains".to_string(), name));
                        }
                    }
                }
                if let Some(ref remove) = fragment.remove_toolchains {
                    for name in remove {
                        resolved_keys.insert(("toolchains".to_string(), name.clone()));
                    }
                }
                if let Some(ref vars) = fragment.variables {
                    for key in vars.keys() {
                        resolved_keys.insert(("variables".to_string(), key.clone()));
                    }
                }
                if let Some(ref cfs) = fragment.copy_files {
                    for cf in cfs {
                        resolved_keys.insert(("copy_files".to_string(), cf.dest.clone()));
                    }
                }
                if let Some(ref remove) = fragment.remove_copy_files {
                    for entry in remove {
                        resolved_keys.insert(("copy_files".to_string(), entry.dest.clone()));
                    }
                }
                if let Some(ref installs) = fragment.install {
                    for i in installs {
                        resolved_keys.insert(("install".to_string(), i.name.clone()));
                    }
                }
                if let Some(ref remove) = fragment.remove_install {
                    for name in remove {
                        resolved_keys.insert(("install".to_string(), name.clone()));
                    }
                }

                if let Err(e) = apply_fragment(&mut result.config, &fragment) {
                    messages::error(&format!(
                        "Failed to apply fragment '{}': {}",
                        frag_path.display(),
                        e
                    ));
                    std::process::exit(1);
                }
                messages::info(&format!(
                    "  Applied fragment: {}",
                    frag_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                messages::error(&format!(
                    "Failed to load fragment '{}': {}",
                    frag_path.display(),
                    e
                ));
                std::process::exit(1);
            }
        }
    }

    // Remove conflicts resolved by fragments
    if !resolved_keys.is_empty() {
        result
            .conflicts
            .retain(|c| !resolved_keys.contains(&(c.section.clone(), c.key.clone())));
    }

    // Report conflicts
    if !result.conflicts.is_empty() {
        messages::error(&format!("Merge conflicts ({}):", result.conflicts.len()));
        for conflict in &result.conflicts {
            messages::error(&format!("  {}", conflict));
        }
        messages::error("Resolve conflicts by renaming items or using a fragment to override");
        std::process::exit(1);
    }

    // Validate the merged config
    let warnings = validate_merged_config(&result.config);
    for warning in &warnings {
        messages::info(&format!("  Warning: {}", warning));
    }

    // Print notes
    for note in &result.notes {
        messages::info(&format!("  Note: {}", note));
    }

    // Format the merged sdk.yml using the clean YAML emitter
    let target_names: Vec<&str> = configs.iter().map(|(n, _, _)| n.as_str()).collect();
    let source_configs: Vec<(&str, &SdkConfig)> = configs
        .iter()
        .map(|(n, cfg, _)| (n.as_str(), cfg))
        .collect();
    let merged_yaml = format_merged_yaml(&result.config, &target_names, &source_configs);

    // Load and merge os-dependencies.yml files
    let mut os_deps_list = Vec::new();
    for (name, _, target_dir) in &configs {
        let os_deps_path = target_dir.join("os-dependencies.yml");
        if os_deps_path.exists() {
            match load_os_dependencies(&os_deps_path) {
                Ok(deps) => {
                    os_deps_list.push((name.clone(), deps));
                }
                Err(e) => {
                    messages::info(&format!(
                        "  Warning: Failed to load os-dependencies.yml from '{}': {}",
                        name, e
                    ));
                }
            }
        }
    }

    // Load and merge python-dependencies.yml files
    let mut python_deps_list = Vec::new();
    for (name, _, target_dir) in &configs {
        let py_deps_path = target_dir.join("python-dependencies.yml");
        if py_deps_path.exists() {
            match load_python_dependencies(&py_deps_path) {
                Ok(deps) => {
                    python_deps_list.push((name.clone(), deps));
                }
                Err(e) => {
                    messages::info(&format!(
                        "  Warning: Failed to load python-dependencies.yml from '{}': {}",
                        name, e
                    ));
                }
            }
        }
    }

    if dry_run {
        messages::status("Dry run — merged configuration:");
        println!("{}", merged_yaml);
        return;
    }

    // Create output directory
    if let Err(e) = std::fs::create_dir_all(output) {
        messages::error(&format!(
            "Failed to create output directory '{}': {}",
            output.display(),
            e
        ));
        std::process::exit(1);
    }

    // Write merged sdk.yml
    let output_file = output.join("sdk.yml");
    if let Err(e) = std::fs::write(&output_file, &merged_yaml) {
        messages::error(&format!("Failed to write {}: {}", output_file.display(), e));
        std::process::exit(1);
    }

    // Write merged os-dependencies.yml
    if !os_deps_list.is_empty() {
        let os_refs: Vec<(&str, &dsdk_cli::config::OsDependencies)> =
            os_deps_list.iter().map(|(n, d)| (n.as_str(), d)).collect();
        let merged_os = merge_os_dependencies(&os_refs);
        let os_yaml = format_os_dependencies_yaml(&merged_os);
        let os_output = output.join("os-dependencies.yml");
        if let Err(e) = std::fs::write(&os_output, &os_yaml) {
            messages::error(&format!("Failed to write {}: {}", os_output.display(), e));
            std::process::exit(1);
        }
        messages::info(&format!(
            "  Merged os-dependencies.yml from {} targets",
            os_deps_list.len()
        ));
    }

    // Write merged python-dependencies.yml
    if !python_deps_list.is_empty() {
        let py_refs: Vec<(&str, &dsdk_cli::config::PythonDependencies)> = python_deps_list
            .iter()
            .map(|(n, d)| (n.as_str(), d))
            .collect();
        let merged_py = merge_python_dependencies(&py_refs);
        let py_yaml = format_python_dependencies_yaml(&merged_py);
        let py_output = output.join("python-dependencies.yml");
        if let Err(e) = std::fs::write(&py_output, &py_yaml) {
            messages::error(&format!("Failed to write {}: {}", py_output.display(), e));
            std::process::exit(1);
        }
        messages::info(&format!(
            "  Merged python-dependencies.yml from {} targets",
            python_deps_list.len()
        ));
    }

    messages::success(&format!(
        "Merged {} targets → {}",
        targets.len(),
        output_file.display()
    ));
    messages::info(&format!(
        "  {} gits, {} toolchains, {} install steps",
        result.config.gits.len(),
        result.config.toolchains.as_ref().map_or(0, |t| t.len()),
        result.config.install.as_ref().map_or(0, |i| i.len()),
    ));
}

fn main() {
    let cli = Cli::parse();

    // Handle version flag first
    if cli.version {
        print_version_info();
        return;
    }

    // Command is now optional, so we need to handle the case where it's None
    let command = match &cli.command {
        Some(cmd) => cmd,
        None => {
            // Show help and check for updates when no command is provided
            Cli::command().print_help().unwrap();
            print_update_notice(spawn_version_check());
            return;
        }
    };

    match command {
        Commands::ListTargets { source, target } => {
            handle_list_targets_command(source.as_deref(), target.as_deref());
        }
        Commands::Init {
            target,
            source,
            version,
            workspace,
            no_mirror,
            force,
            r#match,
            verbose,
            install,
            full,
            no_sudo,
            symlink,
            yes,
            cert_validation,
            fragments,
            no_fragments,
        } => {
            // Validate that target is provided
            let target_name = match target {
                Some(t) => t.clone(),
                None => {
                    messages::error("--target is required");
                    messages::status("Use 'cim list-targets' to see available targets");
                    std::process::exit(1);
                }
            };

            handle_init_command(InitConfig {
                target: target_name,
                source: source.clone(),
                version: version.clone(),
                workspace: workspace.clone(),
                no_mirror: *no_mirror,
                force: *force,
                match_pattern: r#match.as_deref(),
                verbose: *verbose,
                install: *install,
                full: *full,
                no_sudo: *no_sudo,
                symlink: *symlink,
                yes: *yes,
                _cert_validation: cert_validation.as_deref(),
                _fragments: fragments.clone(),
                _no_fragments: *no_fragments,
            });
        }
        Commands::Foreach { command, r#match } => {
            handle_foreach_command(command, r#match.as_deref());
        }
        Commands::Update {
            no_mirror,
            r#match,
            verbose,
            cert_validation,
            fragments,
            no_fragments,
        } => {
            handle_update_command(
                *no_mirror,
                r#match.as_deref(),
                *verbose,
                cert_validation.as_deref(),
                fragments,
                *no_fragments,
            );
        }
        Commands::Makefile { no_dividers } => {
            handle_makefile_command(*no_dividers);
        }
        Commands::Add { name, url, commit } => {
            handle_add_command(name, url, commit);
        }
        Commands::Install { install_command } => {
            handle_install_command(install_command);
        }
        Commands::Docs { docs_command } => {
            handle_docs_command(docs_command);
        }
        Commands::Docker { docker_command } => {
            handle_docker_command(docker_command);
        }
        Commands::Release {
            tag,
            genconfig,
            include,
            exclude,
            dry_run,
        } => {
            handle_release_command(tag.as_deref(), *genconfig, include, exclude, *dry_run);
        }
        Commands::Config {
            list,
            get,
            show_path,
            template,
            create,
            force,
            edit,
            validate,
        } => {
            handle_config_command(ConfigOptions {
                list: *list,
                get: get.as_deref(),
                show_path: *show_path,
                template: *template,
                create: *create,
                force: *force,
                edit: *edit,
                validate: *validate,
            });
        }
        Commands::Utils { utils_command } => {
            handle_utils_command(utils_command);
        }
        Commands::Fragment { fragment_command } => {
            handle_fragment_command(fragment_command);
        }
        Commands::Merge {
            targets,
            output,
            source,
            mirror,
            fragments,
            dry_run,
        } => {
            handle_merge_command(
                targets,
                output,
                source.as_deref(),
                mirror.as_deref(),
                fragments,
                *dry_run,
            );
        }
    }
}
