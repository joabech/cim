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
                let name = path.file_name().unwrap_or_default().to_string_lossy();
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
                        .unwrap_or(&name);
                    if desc.is_empty() {
                        messages::info(&format!("  {}", frag_name));
                    } else {
                        messages::info(&format!("  {} - {}", frag_name, desc));
                    }
                } else {
                    messages::info(&format!("  {} (parse error)", name));
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
        FragmentCommand::Add { path } => {
            if !path.exists() {
                messages::error(&format!("Fragment file not found: {}", path.display()));
                return;
            }

            // Validate the fragment parses correctly
            if let Err(e) = load_fragment(path) {
                messages::error(&format!("Invalid fragment file: {}", e));
                return;
            }

            // Create fragments directory if needed
            if let Err(e) = std::fs::create_dir_all(&fragments_dir) {
                messages::error(&format!("Failed to create fragments directory: {}", e));
                return;
            }

            let filename = path.file_name().unwrap_or_default();
            let dest = fragments_dir.join(filename);
            if dest.exists() {
                messages::error(&format!("Fragment already exists: {}", dest.display()));
                return;
            }

            match std::fs::copy(path, &dest) {
                Ok(_) => {
                    messages::success(&format!("Added fragment: {}", filename.to_string_lossy()));
                }
                Err(e) => {
                    messages::error(&format!("Failed to copy fragment: {}", e));
                }
            }
        }
        FragmentCommand::Remove { name } => {
            // Find the fragment file
            let path = {
                let candidate = fragments_dir.join(format!("{}.fragment.yml", name));
                if candidate.exists() {
                    candidate
                } else {
                    let exact = fragments_dir.join(name);
                    if exact.exists() {
                        exact
                    } else {
                        messages::error(&format!("Fragment not found: {}", name));
                        return;
                    }
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

fn handle_merge_command(
    targets: &[String],
    output: &std::path::Path,
    source: Option<&str>,
    mirror_override: Option<&std::path::Path>,
    fragments: &[std::path::PathBuf],
    dry_run: bool,
) {
    use dsdk_cli::config::{load_config, load_fragment, SdkConfig};
    use dsdk_cli::fragment::{apply_fragment, validate_merged_config};
    use dsdk_cli::merge::merge_targets;
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

    // Load all target configurations
    let mut configs: Vec<(String, SdkConfig)> = Vec::new();
    for target_name in targets {
        let config_path = match init_cmd::resolve_target_config(target_name, config_root) {
            Ok(p) => p,
            Err(e) => {
                messages::error(&format!(
                    "Failed to resolve target '{}': {}",
                    target_name, e
                ));
                std::process::exit(1);
            }
        };

        let config = match load_config(&config_path) {
            Ok(c) => c,
            Err(e) => {
                messages::error(&format!("Failed to load target '{}': {}", target_name, e));
                std::process::exit(1);
            }
        };

        messages::info(&format!(
            "  Loaded '{}' ({} gits)",
            target_name,
            config.gits.len()
        ));
        configs.push((target_name.clone(), config));
    }

    // Build reference pairs for merge
    let target_refs: Vec<(&str, &SdkConfig)> = configs
        .iter()
        .map(|(name, cfg)| (name.as_str(), cfg))
        .collect();

    let mut result = merge_targets(&target_refs, mirror_override);

    // Apply fragments to the merged result
    for frag_path in fragments {
        if !frag_path.exists() {
            messages::error(&format!("Fragment file not found: {}", frag_path.display()));
            std::process::exit(1);
        }
        match load_fragment(frag_path) {
            Ok(fragment) => {
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

    if dry_run {
        messages::status("Dry run — merged configuration:");
        match serde_yaml::to_string(&result.config) {
            Ok(yaml) => println!("{}", yaml),
            Err(e) => {
                messages::error(&format!("Failed to serialize merged config: {}", e));
                std::process::exit(1);
            }
        }
        return;
    }

    // Write the merged sdk.yml
    if let Err(e) = std::fs::create_dir_all(output) {
        messages::error(&format!(
            "Failed to create output directory '{}': {}",
            output.display(),
            e
        ));
        std::process::exit(1);
    }

    let output_file = output.join("sdk.yml");
    match serde_yaml::to_string(&result.config) {
        Ok(yaml) => {
            if let Err(e) = std::fs::write(&output_file, &yaml) {
                messages::error(&format!("Failed to write {}: {}", output_file.display(), e));
                std::process::exit(1);
            }
        }
        Err(e) => {
            messages::error(&format!("Failed to serialize merged config: {}", e));
            std::process::exit(1);
        }
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
