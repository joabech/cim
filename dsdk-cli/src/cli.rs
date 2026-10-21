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

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Flags shared verbatim by `init` and `bootstrap` for selecting/locating a
/// target and controlling how its workspace is cloned. Flattened into both
/// subcommands via `#[command(flatten)]` so the two can never drift apart --
/// adding or changing a flag here automatically applies to both.
#[derive(Args, Debug)]
pub struct WorkspaceSelectionArgs {
    /// Manifest source: git URL or local path
    ///
    /// Defaults to $HOME/devel/cim-manifests.
    #[arg(short, long, value_name = "URL|PATH")]
    pub source: Option<String>,
    /// Target version (branch or tag)
    ///
    /// When omitted, the target's default version is used.
    #[arg(short, long, value_name = "VERSION")]
    pub version: Option<String>,
    /// Workspace directory
    ///
    /// Defaults to $HOME/dsdk-<target>.
    #[arg(short, long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
    /// Skip the mirror and use remote URLs
    #[arg(short = 'N', long)]
    pub no_mirror: bool,
    /// Mirror cache directory
    ///
    /// Overrides the config file and the default location.
    #[arg(short, long, value_name = "DIR")]
    pub mirror: Option<PathBuf>,
    /// Only repositories matching this regex
    #[arg(short = 'p', long, value_name = "REGEX")]
    pub r#match: Option<String>,
    /// Only repositories in these groups
    ///
    /// Comma-separated group names.
    #[arg(short = 'g', long, value_name = "NAMES")]
    pub include_group: Option<String>,
    /// Skip repositories in these groups
    ///
    /// Comma-separated group names.
    #[arg(short = 'G', long, value_name = "NAMES")]
    pub exclude_group: Option<String>,
    /// Show detailed progress information
    #[arg(long)]
    pub verbose: bool,
    /// Skip all confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
    /// Certificate validation mode
    ///
    /// One of: strict (default), relaxed (insecure), auto.
    #[arg(short = 'c', long, value_name = "MODE")]
    pub cert_validation: Option<String>,
}

#[derive(Parser)]
#[command(name = "cim")]
#[command(disable_version_flag = true)]
#[command(
    about = "Code in Motion - Multi-repository workspace manager",
    after_long_help = "WORKFLOW:\n  \
        1. List targets:         cim list-targets [-s DIR|URL]\n  \
        2. Initialize workspace: cim init -t <name> [-w DIR]\n  \
        3. Use from workspace:   cd DIR && cim <command>\n\n\
        CONFIG FILES:\n  \
        sdk.yml                  - Main configuration\n  \
        .workspace               - Workspace marker (auto-created)\n  \
        os-dependencies.yml      - System packages\n  \
        python-dependencies.yml  - Python packages"
)]
pub struct Cli {
    /// Show version
    #[arg(short = 'v', long = "version")]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List available targets and versions
    ListTargets {
        /// Manifest source: git URL or local path
        ///
        /// Defaults to $HOME/devel/cim-manifests.
        #[arg(short, long, value_name = "URL|PATH")]
        source: Option<String>,
        /// List versions for this target
        #[arg(short, long, value_name = "NAME")]
        target: Option<String>,
        /// Show credential attempts and git operations
        #[arg(long)]
        verbose: bool,
        /// Output format: text or json
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// Initialize a new workspace from a manifest
    Init {
        /// Target name or URL
        ///
        /// Looks in targets/<name>/sdk.yml or fetches from the URL.
        #[arg(short, long, value_name = "TARGET")]
        target: Option<String>,
        #[command(flatten)]
        common: WorkspaceSelectionArgs,
        /// Replace an existing workspace directory
        #[arg(short, long)]
        force: bool,
        /// Also install toolchains, pip packages and tools
        #[arg(short, long)]
        install: bool,
        /// Also install OS deps, toolchains, pip and tools
        #[arg(short = 'F', long)]
        full: bool,
        /// Do not use sudo for package installs
        ///
        /// For root users or special configurations.
        #[arg(short = 'S', long = "no-sudo")]
        no_sudo: bool,
        /// Symlink toolchains and pip venv from the mirror
        ///
        /// Requires a mirror.
        #[arg(short = 'l', long)]
        symlink: bool,
    },
    /// Pick a target, init a workspace and build it
    Bootstrap {
        /// Target name or URL
        ///
        /// Omit to pick interactively. Required when not running in a
        /// terminal, e.g. in CI.
        #[arg(short, long, value_name = "TARGET")]
        target: Option<String>,
        #[command(flatten)]
        common: WorkspaceSelectionArgs,
    },
    /// Update all git repositories
    Update {
        /// Skip the mirror and use remote URLs
        #[arg(short = 'N', long)]
        no_mirror: bool,
        /// Mirror cache directory
        ///
        /// Overrides the config file and the default location.
        #[arg(short, long, value_name = "DIR")]
        mirror: Option<PathBuf>,
        /// Only repositories matching this regex
        #[arg(short = 'p', long, value_name = "REGEX")]
        r#match: Option<String>,
        /// Only repositories in these groups
        ///
        /// Comma-separated group names.
        #[arg(short = 'g', long, value_name = "NAMES")]
        include_group: Option<String>,
        /// Skip repositories in these groups
        ///
        /// Comma-separated group names.
        #[arg(short = 'G', long, value_name = "NAMES")]
        exclude_group: Option<String>,
        /// Update all repositories
        ///
        /// Ignores the match filter stored by init.
        #[arg(short, long)]
        all: bool,
        /// Show detailed progress information
        #[arg(long)]
        verbose: bool,
        /// Certificate validation mode
        ///
        /// One of: strict (default), relaxed (insecure), auto.
        #[arg(short = 'c', long, value_name = "MODE")]
        cert_validation: Option<String>,
    },
    /// Execute a command in each repository
    Foreach {
        /// Command to run in each repository
        command: String,
        /// Only repositories matching this regex
        #[arg(short = 'p', long, value_name = "REGEX")]
        r#match: Option<String>,
        /// Only repositories in these groups
        ///
        /// Comma-separated group names.
        #[arg(short = 'g', long, value_name = "NAMES")]
        include_group: Option<String>,
        /// Skip repositories in these groups
        ///
        /// Comma-separated group names.
        #[arg(short = 'G', long, value_name = "NAMES")]
        exclude_group: Option<String>,
    },
    /// Generate a Makefile from the manifest
    Makefile {
        /// Omit section dividers in the Makefile
        #[arg(short = 'd', long)]
        no_dividers: bool,
        /// Only repositories in these groups
        ///
        /// Comma-separated group names.
        #[arg(short = 'g', long, value_name = "NAMES")]
        include_group: Option<String>,
        /// Skip repositories in these groups
        ///
        /// Comma-separated group names.
        #[arg(short = 'G', long, value_name = "NAMES")]
        exclude_group: Option<String>,
    },
    /// Add a git repository to the manifest
    Add {
        /// Repository name
        #[arg(short, long)]
        name: String,
        /// Repository URL
        #[arg(short, long)]
        url: String,
        /// Commit or tag to check out
        #[arg(short, long)]
        commit: String,
    },
    /// Install OS packages, Python packages or toolchains
    Install {
        #[command(subcommand)]
        install_command: InstallCommand,
    },
    /// Create and manage unified documentation
    Docs {
        #[command(subcommand)]
        docs_command: DocsCommand,
    },
    /// Generate Docker configurations
    Docker {
        #[command(subcommand)]
        docker_command: DockerCommand,
    },
    /// Create release tags, configuration and deliverables
    Release {
        /// Release tag to apply, e.g. v1.0.0
        #[arg(short, long)]
        tag: Option<String>,
        /// Generate a release configuration file
        #[arg(short, long)]
        genconfig: bool,
        /// Only repositories matching this regex
        ///
        /// Unanchored, use '^name$' for an exact match. Comma-separated or repeated: --include 'adi.*,core.*' or
        /// --include 'adi.*' --include 'core.*'. --tag only considers the
        /// target's own gits (for an extends: target, the ones it adds or
        /// modifies); --stage and --export-manifest select from all
        /// workspace gits, including those inherited via extends:.
        #[arg(short, long, value_name = "REGEX")]
        include: Vec<String>,
        /// Skip repositories matching this regex
        ///
        /// Unanchored, use '^name$' for an exact match. Comma-separated or repeated: --exclude 'drivers,CMSIS_6' or
        /// --exclude drivers --exclude CMSIS_6.
        #[arg(short, long, value_name = "REGEX")]
        exclude: Vec<String>,
        /// Show what would be done without changes
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Export selected gits' HEAD trees into DIR
        ///
        /// Each git lands in DIR/<git-name>. Uses `git archive`, so only
        /// committed content is staged; refuses
        /// if a selected git has tracked uncommitted changes. Submodule
        /// content is not staged (a warning names them). Refuses if a
        /// destination is or contains the workspace, or overlaps any
        /// workspace git checkout. Existing
        /// DIR/<git-name> folders are replaced, other content of DIR is left
        /// alone. Writes DIR/stage.json with each git's sha and its url as
        /// written in sdk.yml (env vars such as ${TOKEN} not expanded, user
        /// config overrides not applied). stage.json describes this run
        /// only; trees staged by earlier runs into DIR are left in place
        /// and not listed.
        #[arg(short, long, value_name = "DIR")]
        stage: Option<PathBuf>,
        /// Write a cleaned customer sdk.yml to FILE
        ///
        /// Built from the resolved workspace config (an extends:/overlay:
        /// target is flattened), with only the gits selected by
        /// --include/--exclude and without their group: key. Variable
        /// values stay as written in sdk.yml (not expanded); comments are
        /// not kept. Everything else passes through unless dropped below.
        /// Nothing is rewritten to fit: the export is refused, listing each
        /// problem, if a depends_on, build_depends_on, git_depends_on or
        /// depends_on_gits entry would name a git that is not exported or a
        /// dropped phase, or a value or command would reference a variable
        /// that is not exported (${{ VAR }} or $(VAR)).
        #[arg(short = 'm', long, value_name = "FILE")]
        export_manifest: Option<PathBuf>,
        /// URL of each exported git; {name} is the git name
        ///
        /// E.g. --url-template 'https://example.com/acme/{name}.git'.
        #[arg(short, long, value_name = "TEMPLATE", requires = "export_manifest")]
        url_template: Option<String>,
        /// Commit (branch, tag or sha) of each exported git
        #[arg(short, long, value_name = "REF", requires = "export_manifest")]
        commit: Option<String>,
        /// Drop this custom phase from the exported phases:
        ///
        /// Exact phase names, comma-separated or repeated (unlike the
        /// --drop-* regex options): --drop-phase prune,release-verify.
        /// Every name must match an entry of phases:; standard phases
        /// (build, test, ...) cannot be dropped. phases: is omitted when
        /// empty.
        #[arg(
            short = 'p',
            long,
            value_name = "NAME",
            value_delimiter = ',',
            requires = "export_manifest"
        )]
        drop_phase: Vec<String>,
        /// Drop exported variables whose name matches this regex
        ///
        /// Unanchored, use '^NAME$' for an exact match. Repeated, not
        /// comma-separated: --drop-var '^RELEASE_' --drop-var '^CUSTOMER$'.
        #[arg(short = 'V', long, value_name = "REGEX", requires = "export_manifest")]
        drop_var: Vec<String>,
        /// Drop exported copy_files whose source matches this regex
        ///
        /// Unanchored. Repeated, not comma-separated. copy_files: is
        /// omitted when empty.
        #[arg(short = 'F', long, value_name = "REGEX", requires = "export_manifest")]
        drop_copy_files: Vec<String>,
    },

    /// Manage user configuration
    Config {
        /// List all configuration values
        #[arg(short = 'l', long = "list")]
        list: bool,

        /// Get a configuration value
        #[arg(short = 'g', long = "get", value_name = "KEY")]
        get: Option<String>,

        /// Show config file location
        #[arg(short = 'p', long = "path")]
        show_path: bool,

        /// Print config template to stdout
        #[arg(short = 't', long = "template")]
        template: bool,

        /// Create config file from template
        #[arg(short = 'c', long = "create")]
        create: bool,

        /// Overwrite when creating config
        #[arg(short = 'f', long = "force")]
        force: bool,

        /// Open config in editor
        ///
        /// Creates the file from the template if it is missing.
        #[arg(short = 'e', long = "edit")]
        edit: bool,

        /// Validate config file syntax
        #[arg(short = 'v', long = "validate")]
        validate: bool,
    },

    /// Workspace maintenance utilities
    Utils {
        #[command(subcommand)]
        utils_command: UtilsCommand,
    },
}

#[derive(Subcommand)]
pub enum DocsCommand {
    /// Aggregate all repository docs into one tree
    Create {
        /// Recreate documentation even if it exists
        #[arg(short, long)]
        force: bool,
        /// Sphinx theme
        #[arg(short, long, default_value = "sphinx_rtd_theme")]
        theme: String,
        /// Symlink files instead of copying (Linux/macOS)
        #[arg(short = 'l', long)]
        symlink: bool,
        /// Show detailed search information
        #[arg(long)]
        verbose: bool,
        /// Certificate validation mode
        ///
        /// One of: strict (default), relaxed (insecure), auto.
        #[arg(short = 'c', long, value_name = "MODE")]
        cert_validation: Option<String>,
    },
    /// Build the unified documentation
    Build {
        /// Output format: html, pdf or epub
        #[arg(short, long, default_value = "html")]
        format: String,
    },
    /// Serve documentation locally with live reload
    Serve {
        /// Port to serve on
        #[arg(short, long, default_value = "8000")]
        port: u16,
        /// Host to bind to
        #[arg(short = 'H', long, default_value = "localhost")]
        host: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DockerCommand {
    /// Create a Dockerfile for SDK development
    Create {
        /// Target name
        ///
        /// Looks in targets/<name>/sdk.yml.
        #[arg(short, long, value_name = "TARGET")]
        target: String,
        /// Manifest source: git URL or local path
        ///
        /// Defaults to $HOME/devel/cim-manifests.
        #[arg(short, long, value_name = "URL|PATH")]
        source: Option<String>,
        /// Target version (branch or tag)
        #[arg(short, long, value_name = "VERSION")]
        version: Option<String>,
        /// Base image, e.g. ubuntu:22.04
        ///
        /// Defaults to ubuntu:22.04.
        #[arg(short, long, value_name = "IMAGE")]
        distro: Option<String>,
        /// Output Dockerfile path
        #[arg(short, long, default_value = "Dockerfile")]
        output: PathBuf,
        /// Overwrite an existing Dockerfile
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum InstallCommand {
    /// Install OS system packages
    OsDeps {
        /// Skip the confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Do not use sudo for package installs
        ///
        /// For root users or special configurations.
        #[arg(short = 'S', long = "no-sudo")]
        no_sudo: bool,
    },
    /// Install Python packages
    Pip {
        /// Remove the existing venv and reinstall
        #[arg(short, long)]
        force: bool,
        /// Use a shared venv in the mirror via symlink
        #[arg(short = 'l', long)]
        symlink: bool,
        /// Dependency profile to use
        ///
        /// E.g. minimal, docs, dev, full. Comma-separated for several:
        /// --profile dev,docs
        #[arg(short, long, value_name = "PROFILE")]
        profile: Option<String>,
        /// List available dependency profiles
        #[arg(short = 'L', long)]
        list_profiles: bool,
        /// Only deps for repositories in these groups
        ///
        /// Limits per-repo Python deps to repositories in these
        /// comma-separated groups.
        #[arg(short = 'g', long, value_name = "NAMES")]
        include_group: Option<String>,
        /// Skip deps for repositories in these groups
        ///
        /// Excludes per-repo Python deps for repositories in these
        /// comma-separated groups.
        #[arg(short = 'G', long, value_name = "NAMES")]
        exclude_group: Option<String>,
        /// Certificate validation mode
        ///
        /// One of: strict (default), relaxed (insecure), auto.
        #[arg(short = 'c', long, value_name = "MODE")]
        cert_validation: Option<String>,
        /// Repair the shared mirror venv in place
        ///
        /// Repairs the shared mirror virtual environment (used by
        /// --symlink workspaces) instead of installing packages. Tries a
        /// non-destructive fix first (e.g. a stale interpreter path after
        /// a system Python upgrade) and only rebuilds it from scratch as
        /// a last resort.
        #[arg(
            short,
            long,
            conflicts_with_all = [
                "force",
                "symlink",
                "profile",
                "list_profiles",
                "include_group",
                "exclude_group"
            ]
        )]
        repair: bool,
    },
    /// Install and extract toolchains
    Toolchains {
        /// Remove existing destinations and reinstall
        #[arg(short, long)]
        force: bool,
        /// Install to the mirror and symlink in workspace
        #[arg(short = 'l', long)]
        symlink: bool,
        /// Show detailed progress information
        #[arg(long)]
        verbose: bool,
        /// Certificate validation mode
        ///
        /// One of: strict (default), relaxed (insecure), auto.
        #[arg(short = 'c', long, value_name = "MODE")]
        cert_validation: Option<String>,
    },
    /// Install SDK tools from the install section
    Tools {
        /// Component to install
        ///
        /// E.g. ninja, ccache, zephyr-sdk. Wraps 'make install-<name>',
        /// which can also be run directly.
        name: Option<String>,
        /// List available install targets
        #[arg(short = 'l', long)]
        list: bool,
        /// Install all components
        #[arg(short, long)]
        all: bool,
        /// Remove the sentinel file and reinstall
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum UtilsCommand {
    /// Update SHA256 hashes of copy_files entries
    HashCopyFiles {
        /// File to update (dest path or source URL)
        ///
        /// Updates all entries when omitted.
        #[arg(value_name = "FILE")]
        file: Option<String>,
        /// Show proposed changes without updating files
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Show detailed progress information
        #[arg(long)]
        verbose: bool,
        /// Add missing sha256 fields instead of skipping
        ///
        /// Entries without a sha256 field get one, then the hash is
        /// computed and set.
        #[arg(short, long)]
        add_missing: bool,
    },
    /// Update SHA256 hashes of toolchain entries
    HashToolchains {
        /// Toolchain to update (name or URL)
        ///
        /// Updates all entries when omitted.
        #[arg(value_name = "TOOLCHAIN")]
        file: Option<String>,
        /// Show proposed changes without updating files
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Show detailed progress information
        #[arg(long)]
        verbose: bool,
        /// Add missing sha256 fields instead of skipping
        ///
        /// Entries without a sha256 field get one, then the hash is
        /// computed and set.
        #[arg(short, long)]
        add_missing: bool,
    },
    /// Re-sync copy_files entries into the workspace
    SyncCopyFiles {
        /// File to sync (dest path or source URL)
        ///
        /// Syncs all entries when omitted.
        #[arg(value_name = "FILE")]
        file: Option<String>,
        /// Show proposed changes without copying files
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Show detailed progress information
        #[arg(long)]
        verbose: bool,
        /// Overwrite existing files
        #[arg(short, long)]
        force: bool,
    },
    /// Update cim to the latest release
    Update,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Command, CommandFactory, Parser};

    fn visit(cmd: &Command, f: &mut dyn FnMut(&Command)) {
        f(cmd);
        for sub in cmd.get_subcommands() {
            visit(sub, f);
        }
    }

    #[test]
    fn test_every_option_has_short_form_except_verbose() {
        let mut missing = Vec::new();
        visit(&Cli::command(), &mut |cmd| {
            for arg in cmd.get_arguments() {
                if !arg.is_positional() && arg.get_short().is_none() && arg.get_id() != "verbose" {
                    missing.push(format!("{} --{}", cmd.get_name(), arg.get_id()));
                }
            }
        });
        assert!(
            missing.is_empty(),
            "options without short form: {missing:?}"
        );
    }

    #[test]
    fn test_short_help_does_not_wrap_at_80_columns() {
        let mut wrapped = Vec::new();
        visit(&Cli::command(), &mut |cmd| {
            let help = cmd.clone().term_width(80).render_help().to_string();
            for line in help.lines() {
                let indent = line.len() - line.trim_start().len();
                if indent >= 14 && !line.trim().is_empty() {
                    wrapped.push(format!("{}: {}", cmd.get_name(), line.trim()));
                }
            }
        });
        assert!(wrapped.is_empty(), "wrapped help lines: {wrapped:#?}");
    }

    #[test]
    fn test_new_cli_structure_compatibility() {
        // Test that the new CLI structure maintains expected functionality

        // Test list-targets command parsing
        let list_args = vec!["cim", "list-targets"];
        let cli_result = Cli::try_parse_from(list_args);
        assert!(cli_result.is_ok());

        let cli = cli_result.unwrap();
        match &cli.command {
            Some(Commands::ListTargets {
                source,
                target,
                verbose,
                format,
            }) => {
                assert!(source.is_none());
                assert!(target.is_none());
                assert!(!verbose);
                assert_eq!(format, "text");
            }
            _ => panic!("Expected ListTargets command"),
        }

        // Test list-targets with source
        let list_args_with_source = vec!["cim", "list-targets", "--source", "/path/to/source"];
        let cli_result = Cli::try_parse_from(list_args_with_source);
        assert!(cli_result.is_ok());

        let cli = cli_result.unwrap();
        match &cli.command {
            Some(Commands::ListTargets {
                source,
                target,
                verbose,
                format,
            }) => {
                assert_eq!(source, &Some("/path/to/source".to_string()));
                assert!(target.is_none());
                assert!(!verbose);
                assert_eq!(format, "text");
            }
            _ => panic!("Expected ListTargets command"),
        }
    }

    #[test]
    fn test_updated_init_cli_structure() {
        use clap::Parser;

        // Test init command with new structure
        let init_args = vec![
            "cim",
            "init",
            "--target",
            "my-target",
            "--source",
            "https://github.com/user/repo.git",
            "--version",
            "v1.0.0",
        ];
        let cli_result = Cli::try_parse_from(init_args);
        assert!(cli_result.is_ok());

        let cli = cli_result.unwrap();
        match &cli.command {
            Some(Commands::Init {
                target,
                common,
                force: _,
                install: _,
                full: _,
                no_sudo: _,
                symlink: _,
            }) => {
                assert_eq!(target, &Some("my-target".to_string()));
                assert_eq!(
                    common.source,
                    Some("https://github.com/user/repo.git".to_string())
                );
                assert_eq!(common.version, Some("v1.0.0".to_string()));
            }
            _ => panic!("Expected Init command"),
        }

        // Test that --list-targets option is no longer available in init command
        let invalid_args = vec!["cim", "init", "--list-targets"];
        let cli_result = Cli::try_parse_from(invalid_args);
        assert!(cli_result.is_err()); // Should fail because --list-targets is removed from init
    }

    #[test]
    fn test_bootstrap_cli_structure() {
        use clap::Parser;

        // No flags: interactive picker path (target/version/source all None)
        let cli_result = Cli::try_parse_from(vec!["cim", "bootstrap"]);
        assert!(cli_result.is_ok());
        let cli = cli_result.unwrap();
        match &cli.command {
            Some(Commands::Bootstrap { target, common }) => {
                assert!(target.is_none());
                assert!(common.source.is_none());
                assert!(common.version.is_none());
            }
            _ => panic!("Expected Bootstrap command"),
        }

        // Explicit target/version/source: fully non-interactive/CI-friendly path
        let cli_result = Cli::try_parse_from(vec![
            "cim",
            "bootstrap",
            "--target",
            "my-target",
            "--source",
            "https://github.com/user/repo.git",
            "--version",
            "v1.0.0",
            "--yes",
        ]);
        assert!(cli_result.is_ok());
        let cli = cli_result.unwrap();
        match &cli.command {
            Some(Commands::Bootstrap { target, common }) => {
                assert_eq!(target, &Some("my-target".to_string()));
                assert_eq!(
                    common.source,
                    Some("https://github.com/user/repo.git".to_string())
                );
                assert_eq!(common.version, Some("v1.0.0".to_string()));
                assert!(common.yes);
            }
            _ => panic!("Expected Bootstrap command"),
        }
    }
}
