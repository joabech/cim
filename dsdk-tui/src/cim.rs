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

use anyhow::{anyhow, Result};
use std::process::Command;
use tokio::process::Command as TokioCommand;

pub fn fetch_targets(source: &str) -> Result<Vec<String>> {
    let mut cmd = Command::new("cim");
    cmd.arg("list-targets");

    if !source.is_empty() {
        cmd.args(["--source", source]);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to fetch targets: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut targets = Vec::new();

    for line in stdout.lines() {
        if let Some(target) = line.strip_prefix("  - ") {
            targets.push(target.trim().to_string());
        }
    }

    Ok(targets)
}

pub fn fetch_versions(source: &str, target: &str) -> Result<Vec<String>> {
    let mut cmd = Command::new("cim");
    cmd.arg("list-targets");
    cmd.args(["--target", target]);

    if !source.is_empty() {
        cmd.args(["--source", source]);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to fetch versions: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut versions = Vec::new();

    for line in stdout.lines() {
        if let Some(version) = line.strip_prefix("  - ") {
            versions.push(version.trim().to_string());
        }
    }

    Ok(versions)
}

#[allow(clippy::large_enum_variant)]
pub enum CimCommand {
    Init {
        target: String,
        source: String,
        version: Option<String>,
        workspace: Option<String>,
        no_mirror: bool,
        force: bool,
        match_pattern: Option<String>,
        verbose: bool,
        install: bool,
        full: bool,
        no_sudo: bool,
        symlink: bool,
        yes: bool,
        cert_validation: Option<String>,
    },
    Update {
        no_mirror: bool,
        match_pattern: Option<String>,
        verbose: bool,
        cert_validation: Option<String>,
    },
    Foreach {
        command: String,
        match_pattern: Option<String>,
    },
    Makefile {
        no_dividers: bool,
    },
    Add {
        name: String,
        url: String,
        commit: String,
    },
    InstallOsDeps {
        yes: bool,
        no_sudo: bool,
    },
    InstallPip {
        force: bool,
        symlink: bool,
        profile: Option<String>,
        list_profiles: bool,
    },
    InstallToolchains {
        force: bool,
        symlink: bool,
        verbose: bool,
    },
    InstallTools {
        name: Option<String>,
        list: bool,
        all: bool,
        force: bool,
    },
    DocsCreate {
        force: bool,
        theme: String,
        symlink: bool,
        verbose: bool,
    },
    DocsBuild {
        format: String,
    },
    DocsServe {
        port: u16,
        host: String,
    },
    Release {
        tag: Option<String>,
        genconfig: bool,
        include: Vec<String>,
        exclude: Vec<String>,
        dry_run: bool,
    },
    Config {
        list: bool,
        get: Option<String>,
        show_path: bool,
        template: bool,
        create: bool,
        force: bool,
        edit: bool,
        validate: bool,
    },
    UtilsHashCopyFiles {
        file: Option<String>,
        dry_run: bool,
        verbose: bool,
        add_missing: bool,
    },
    UtilsHashToolchains {
        file: Option<String>,
        dry_run: bool,
        verbose: bool,
        add_missing: bool,
    },
    UtilsSyncCopyFiles {
        file: Option<String>,
        dry_run: bool,
        verbose: bool,
        force: bool,
    },
    UtilsUpdate,
}

impl CimCommand {
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        match self {
            CimCommand::Init {
                target,
                source,
                version,
                workspace,
                no_mirror,
                force,
                match_pattern,
                verbose,
                install,
                full,
                no_sudo,
                symlink,
                yes,
                cert_validation,
            } => {
                args.push("init".into());
                args.extend(["--target".into(), target.clone()]);
                if !source.is_empty() {
                    args.extend(["--source".into(), source.clone()]);
                }
                if let Some(v) = version {
                    args.extend(["--version".into(), v.clone()]);
                }
                if let Some(w) = workspace {
                    if !w.is_empty() {
                        args.extend(["--workspace".into(), w.clone()]);
                    }
                }
                if *no_mirror {
                    args.push("--no-mirror".into());
                }
                if *force {
                    args.push("--force".into());
                }
                if let Some(m) = match_pattern {
                    if !m.is_empty() {
                        args.extend(["--match".into(), m.clone()]);
                    }
                }
                if *verbose {
                    args.push("--verbose".into());
                }
                if *install {
                    args.push("--install".into());
                }
                if *full {
                    args.push("--full".into());
                }
                if *no_sudo {
                    args.push("--no-sudo".into());
                }
                if *symlink {
                    args.push("--symlink".into());
                }
                if *yes {
                    args.push("--yes".into());
                }
                if let Some(cv) = cert_validation {
                    if !cv.is_empty() {
                        args.extend(["--cert-validation".into(), cv.clone()]);
                    }
                }
            }
            CimCommand::Update {
                no_mirror,
                match_pattern,
                verbose,
                cert_validation,
            } => {
                args.push("update".into());
                if *no_mirror {
                    args.push("--no-mirror".into());
                }
                if let Some(m) = match_pattern {
                    if !m.is_empty() {
                        args.extend(["--match".into(), m.clone()]);
                    }
                }
                if *verbose {
                    args.push("--verbose".into());
                }
                if let Some(cv) = cert_validation {
                    if !cv.is_empty() {
                        args.extend(["--cert-validation".into(), cv.clone()]);
                    }
                }
            }
            CimCommand::Foreach {
                command,
                match_pattern,
            } => {
                args.push("foreach".into());
                args.push(command.clone());
                if let Some(m) = match_pattern {
                    if !m.is_empty() {
                        args.extend(["--match".into(), m.clone()]);
                    }
                }
            }
            CimCommand::Makefile { no_dividers } => {
                args.push("makefile".into());
                if *no_dividers {
                    args.push("--no-dividers".into());
                }
            }
            CimCommand::Add { name, url, commit } => {
                args.push("add".into());
                args.extend(["--name".into(), name.clone()]);
                args.extend(["--url".into(), url.clone()]);
                args.extend(["--commit".into(), commit.clone()]);
            }
            CimCommand::InstallOsDeps { yes, no_sudo } => {
                args.extend(["install".into(), "os-deps".into()]);
                if *yes {
                    args.push("--yes".into());
                }
                if *no_sudo {
                    args.push("--no-sudo".into());
                }
            }
            CimCommand::InstallPip {
                force,
                symlink,
                profile,
                list_profiles,
            } => {
                args.extend(["install".into(), "pip".into()]);
                if *force {
                    args.push("--force".into());
                }
                if *symlink {
                    args.push("--symlink".into());
                }
                if let Some(p) = profile {
                    args.extend(["--profile".into(), p.clone()]);
                }
                if *list_profiles {
                    args.push("--list-profiles".into());
                }
            }
            CimCommand::InstallToolchains {
                force,
                symlink,
                verbose,
            } => {
                args.extend(["install".into(), "toolchains".into()]);
                if *force {
                    args.push("--force".into());
                }
                if *symlink {
                    args.push("--symlink".into());
                }
                if *verbose {
                    args.push("--verbose".into());
                }
            }
            CimCommand::InstallTools {
                name,
                list,
                all,
                force,
            } => {
                args.extend(["install".into(), "tools".into()]);
                if let Some(n) = name {
                    args.push(n.clone());
                }
                if *list {
                    args.push("--list".into());
                }
                if *all {
                    args.push("--all".into());
                }
                if *force {
                    args.push("--force".into());
                }
            }
            CimCommand::DocsCreate {
                force,
                theme,
                symlink,
                verbose,
            } => {
                args.extend(["docs".into(), "create".into()]);
                if *force {
                    args.push("--force".into());
                }
                if !theme.is_empty() && theme != "sphinx_rtd_theme" {
                    args.extend(["--theme".into(), theme.clone()]);
                }
                if *symlink {
                    args.push("--symlink".into());
                }
                if *verbose {
                    args.push("--verbose".into());
                }
            }
            CimCommand::DocsBuild { format } => {
                args.extend(["docs".into(), "build".into()]);
                if !format.is_empty() && format != "html" {
                    args.extend(["--format".into(), format.clone()]);
                }
            }
            CimCommand::DocsServe { port, host } => {
                args.extend(["docs".into(), "serve".into()]);
                if *port != 8000 {
                    args.extend(["--port".into(), port.to_string()]);
                }
                if host != "localhost" {
                    args.extend(["--host".into(), host.clone()]);
                }
            }
            CimCommand::Release {
                tag,
                genconfig,
                include,
                exclude,
                dry_run,
            } => {
                args.push("release".into());
                if let Some(t) = tag {
                    args.extend(["--tag".into(), t.clone()]);
                }
                if *genconfig {
                    args.push("--genconfig".into());
                }
                for i in include {
                    args.extend(["--include".into(), i.clone()]);
                }
                for e in exclude {
                    args.extend(["--exclude".into(), e.clone()]);
                }
                if *dry_run {
                    args.push("--dry-run".into());
                }
            }
            CimCommand::Config {
                list,
                get,
                show_path,
                template,
                create,
                force,
                edit,
                validate,
            } => {
                args.push("config".into());
                if *list {
                    args.push("--list".into());
                }
                if let Some(k) = get {
                    args.extend(["--get".into(), k.clone()]);
                }
                if *show_path {
                    args.push("--path".into());
                }
                if *template {
                    args.push("--template".into());
                }
                if *create {
                    args.push("--create".into());
                }
                if *force {
                    args.push("--force".into());
                }
                if *edit {
                    args.push("--edit".into());
                }
                if *validate {
                    args.push("--validate".into());
                }
            }
            CimCommand::UtilsHashCopyFiles {
                file,
                dry_run,
                verbose,
                add_missing,
            } => {
                args.extend(["utils".into(), "hash-copy-files".into()]);
                if let Some(f) = file {
                    args.push(f.clone());
                }
                if *dry_run {
                    args.push("--dry-run".into());
                }
                if *verbose {
                    args.push("--verbose".into());
                }
                if *add_missing {
                    args.push("--add-missing".into());
                }
            }
            CimCommand::UtilsHashToolchains {
                file,
                dry_run,
                verbose,
                add_missing,
            } => {
                args.extend(["utils".into(), "hash-toolchains".into()]);
                if let Some(f) = file {
                    args.push(f.clone());
                }
                if *dry_run {
                    args.push("--dry-run".into());
                }
                if *verbose {
                    args.push("--verbose".into());
                }
                if *add_missing {
                    args.push("--add-missing".into());
                }
            }
            CimCommand::UtilsSyncCopyFiles {
                file,
                dry_run,
                verbose,
                force,
            } => {
                args.extend(["utils".into(), "sync-copy-files".into()]);
                if let Some(f) = file {
                    args.push(f.clone());
                }
                if *dry_run {
                    args.push("--dry-run".into());
                }
                if *verbose {
                    args.push("--verbose".into());
                }
                if *force {
                    args.push("--force".into());
                }
            }
            CimCommand::UtilsUpdate => {
                args.extend(["utils".into(), "update".into()]);
            }
        }
        args
    }

    pub fn label(&self) -> &str {
        match self {
            CimCommand::Init { .. } => "init",
            CimCommand::Update { .. } => "update",
            CimCommand::Foreach { .. } => "foreach",
            CimCommand::Makefile { .. } => "makefile",
            CimCommand::Add { .. } => "add",
            CimCommand::InstallOsDeps { .. } => "install os-deps",
            CimCommand::InstallPip { .. } => "install pip",
            CimCommand::InstallToolchains { .. } => "install toolchains",
            CimCommand::InstallTools { .. } => "install tools",
            CimCommand::DocsCreate { .. } => "docs create",
            CimCommand::DocsBuild { .. } => "docs build",
            CimCommand::DocsServe { .. } => "docs serve",
            CimCommand::Release { .. } => "release",
            CimCommand::Config { .. } => "config",
            CimCommand::UtilsHashCopyFiles { .. } => "utils hash-copy-files",
            CimCommand::UtilsHashToolchains { .. } => "utils hash-toolchains",
            CimCommand::UtilsSyncCopyFiles { .. } => "utils sync-copy-files",
            CimCommand::UtilsUpdate => "utils update",
        }
    }
}

pub fn spawn_command(cmd: &CimCommand) -> Result<tokio::process::Child> {
    let args = cmd.to_args();
    let mut command = TokioCommand::new("cim");
    command.args(&args);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn cim: {}", e))
}
