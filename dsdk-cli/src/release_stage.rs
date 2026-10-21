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

//! `cim release --stage <DIR>`: export the HEAD tree of each selected
//! workspace git into `<DIR>/<git-name>/` and record what was staged in
//! `<DIR>/stage.json`.

use dsdk_cli::{git_operations, messages};
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Name of the record written next to the staged trees.
pub(crate) const STAGE_RECORD_FILE: &str = "stage.json";

/// One git to stage: its sdk.yml name and url, and its checkout path.
#[derive(Debug, Clone)]
pub(crate) struct StageGit {
    pub name: String,
    pub url: String,
    pub path: PathBuf,
}

/// What a staging destination must never touch: the workspace root and
/// the checkout of every git in sdk.yml, selected for staging or not.
#[derive(Debug, Clone)]
pub(crate) struct StageWorkspace {
    pub root: PathBuf,
    /// `(git name, checkout path)` of every workspace git.
    pub checkouts: Vec<(String, PathBuf)>,
}

/// Destination problems of staging `gits` into `stage_dir`: a destination
/// must not be the workspace root or contain it, and must not be, contain
/// or be inside any workspace git checkout (replacing the destination
/// would delete it).
fn overlap_problems(gits: &[StageGit], ws: &StageWorkspace, stage_dir: &Path) -> Vec<String> {
    let stage_abs = normalize_path(stage_dir);
    let root_abs = normalize_path(&ws.root);
    let mut checkouts: Vec<(&str, PathBuf)> = ws
        .checkouts
        .iter()
        .map(|(n, p)| (n.as_str(), normalize_path(p)))
        .collect();
    for git in gits {
        if !ws.checkouts.iter().any(|(n, _)| n == &git.name) {
            checkouts.push((git.name.as_str(), normalize_path(&git.path)));
        }
    }

    let mut problems = Vec::new();
    for git in gits {
        let dest_abs = stage_abs.join(&git.name);
        if root_abs.starts_with(&dest_abs) {
            problems.push(format!(
                "{}: staging destination {} would replace the workspace {}",
                git.name,
                dest_abs.display(),
                root_abs.display()
            ));
            continue;
        }
        for (name, repo_abs) in &checkouts {
            if dest_abs.starts_with(repo_abs) || repo_abs.starts_with(&dest_abs) {
                problems.push(format!(
                    "{}: staging destination {} overlaps the checkout of git '{}' ({})",
                    git.name,
                    dest_abs.display(),
                    name,
                    repo_abs.display()
                ));
            }
        }
    }
    problems
}

#[derive(Serialize)]
struct StageEntry<'a> {
    sha: &'a str,
    url: &'a str,
}

/// `gits:` map of stage.json. serde_json's own map type is sorted, so
/// serialize by hand to keep sdk.yml order.
struct StageGits<'a>(Vec<(&'a str, StageEntry<'a>)>);

impl Serialize for StageGits<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, entry) in &self.0 {
            map.serialize_entry(name, entry)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct StageRecord<'a> {
    gits: StageGits<'a>,
}

/// Render the stage.json content: pretty-printed, trailing newline,
/// gits in the order given.
fn render_stage_record(gits: &[StageGit], shas: &[String]) -> Result<String, String> {
    let record = StageRecord {
        gits: StageGits(
            gits.iter()
                .zip(shas)
                .map(|(g, sha)| {
                    (
                        g.name.as_str(),
                        StageEntry {
                            sha: sha.as_str(),
                            url: g.url.as_str(),
                        },
                    )
                })
                .collect(),
        ),
    };
    let mut out = serde_json::to_string_pretty(&record)
        .map_err(|e| format!("Failed to serialize {}: {}", STAGE_RECORD_FILE, e))?;
    out.push('\n');
    Ok(out)
}

fn git_stdout(repo: &Path, args: &[&str]) -> Result<String, String> {
    let result = git_operations::git_command(args, Some(repo))
        .map_err(|e| format!("git {} failed in {}: {}", args.join(" "), repo.display(), e))?;
    if !result.success {
        return Err(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo.display(),
            result.stderr.trim()
        ));
    }
    Ok(result.stdout)
}

/// Absolute, symlink-resolved form of `path` as far as it exists; the
/// non-existing tail is appended as-is.
fn normalize_path(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = abs.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(canon) = existing.canonicalize() {
            let mut out = canon;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                existing = parent;
            }
            _ => return abs,
        }
    }
}

/// Check every git before anything is written: no destination may
/// overlap the workspace or a git checkout (see [`overlap_problems`]),
/// and each git must be a git checkout with no tracked uncommitted
/// changes (`git archive` exports HEAD, so they would silently be left
/// out). Returns the HEAD sha of each git.
fn preflight(
    gits: &[StageGit],
    ws: &StageWorkspace,
    stage_dir: &Path,
) -> Result<Vec<String>, String> {
    let mut problems = overlap_problems(gits, ws, stage_dir);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let mut dirty = Vec::new();
    let mut shas = Vec::new();

    for git in gits {
        if !git.path.join(".git").exists() {
            problems.push(format!(
                "{}: {} is not a git checkout",
                git.name,
                git.path.display()
            ));
            continue;
        }
        let status = git_stdout(
            &git.path,
            &["status", "--porcelain", "--untracked-files=no"],
        )?;
        if !status.trim().is_empty() {
            dirty.push(git.name.clone());
        }
        shas.push(
            git_stdout(&git.path, &["rev-parse", "HEAD"])?
                .trim()
                .to_string(),
        );
    }

    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    if !dirty.is_empty() {
        return Err(format!(
            "uncommitted changes in: {}. Commit or stash them; staging uses HEAD.",
            dirty.join(", ")
        ));
    }
    Ok(shas)
}

/// One warning per git whose staged tree has submodules (gitlinks):
/// `git archive` leaves an empty directory for them, so their content is
/// not staged. A gitlink that is itself one of the staged gits (a nested
/// git listed in sdk.yml) is staged on its own and not reported.
fn submodule_warnings(gits: &[StageGit], shas: &[String]) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    for (git, sha) in gits.iter().zip(shas) {
        let tree = git_stdout(&git.path, &["ls-tree", "-r", "-z", sha])?;
        let submodules: Vec<&str> = tree
            .split('\0')
            .filter_map(|entry| {
                let (meta, path) = entry.split_once('\t')?;
                meta.starts_with("160000 ").then_some(path)
            })
            .filter(|path| {
                let nested = format!("{}/{}", git.name, path);
                !gits.iter().any(|g| g.name == nested)
            })
            .collect();
        if !submodules.is_empty() {
            warnings.push(format!(
                "Warning: {} has submodules whose content is not staged: {}",
                git.name,
                submodules.join(", ")
            ));
        }
    }
    Ok(warnings)
}

fn remove_existing(dest: &Path) -> Result<(), String> {
    let Ok(meta) = fs::symlink_metadata(dest) else {
        return Ok(());
    };
    let res = if meta.is_dir() {
        fs::remove_dir_all(dest)
    } else {
        fs::remove_file(dest)
    };
    res.map_err(|e| format!("Failed to remove {}: {}", dest.display(), e))
}

/// Unpack every entry under `dest`, then apply directory mtimes (tar
/// crate leaves those at "now", and they would change again as soon as an
/// entry is written into them anyway).
fn unpack_archive<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    dest: &Path,
) -> std::io::Result<()> {
    let mut dir_mtimes = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_dir() {
            if let Ok(mtime) = entry.header().mtime() {
                dir_mtimes.push((entry.path()?.into_owned(), mtime));
            }
        }
        entry.unpack_in(dest)?;
    }
    for (rel, mtime) in dir_mtimes {
        set_dir_mtime(&dest.join(rel), mtime);
    }
    Ok(())
}

#[cfg(unix)]
fn set_dir_mtime(dir: &Path, mtime: u64) {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime);
    if let Ok(f) = fs::File::open(dir) {
        let _ = f.set_modified(time);
    }
}

#[cfg(not(unix))]
fn set_dir_mtime(_dir: &Path, _mtime: u64) {}

/// `git archive --format=tar <sha>` of `repo`, unpacked into `dest`.
///
/// File modes are those of the archive minus group/other write, i.e. what
/// `git archive | tar x` gives with the usual 022 umask, independent of the
/// caller's umask. Symlinks are kept as symlinks. Files and directories get
/// the archive's mtime (the commit time), as with `tar x`.
fn export_tree(repo: &Path, sha: &str, dest: &Path) -> Result<(), String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["archive", "--format=tar", sha])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to run git archive in {}: {}", repo.display(), e))?;

    let stdout = child.stdout.take().expect("piped stdout");
    let mut archive = tar::Archive::new(stdout);
    archive.set_mask(0o022);
    archive.set_preserve_mtime(true);
    let unpacked = unpack_archive(&mut archive, dest);
    // Drain what's left so git doesn't die on a closed pipe before we wait.
    let mut rest = archive.into_inner();
    let _ = std::io::copy(&mut rest, &mut std::io::sink());

    let status = child
        .wait()
        .map_err(|e| format!("git archive failed in {}: {}", repo.display(), e))?;
    unpacked.map_err(|e| format!("Failed to unpack into {}: {}", dest.display(), e))?;
    if !status.success() {
        return Err(format!("git archive failed in {}", repo.display()));
    }
    Ok(())
}

/// Stage `gits` into `stage_dir`. Each `<stage_dir>/<name>` is replaced;
/// anything else in `stage_dir` is left alone. With `dry_run` only report
/// what would be done.
pub(crate) fn stage_gits(
    gits: &[StageGit],
    ws: &StageWorkspace,
    stage_dir: &Path,
    dry_run: bool,
) -> Result<(), String> {
    if gits.is_empty() {
        return Err("No gits selected for staging".to_string());
    }
    let shas = preflight(gits, ws, stage_dir)?;
    let record = render_stage_record(gits, &shas)?;
    let warnings = submodule_warnings(gits, &shas)?;
    for warning in &warnings {
        messages::info(warning);
    }
    let record_path = stage_dir.join(STAGE_RECORD_FILE);

    if dry_run {
        messages::status(&format!(
            "\nWould stage {} git(s) into {}:",
            gits.len(),
            stage_dir.display()
        ));
        for (git, sha) in gits.iter().zip(&shas) {
            messages::info(&format!(
                "Would stage {} @ {} -> {}",
                git.name,
                &sha[..sha.len().min(12)],
                stage_dir.join(&git.name).display()
            ));
        }
        messages::info(&format!("Would write {}", record_path.display()));
        return Ok(());
    }

    messages::status(&format!(
        "\nStaging {} git(s) into {}...",
        gits.len(),
        stage_dir.display()
    ));
    fs::create_dir_all(stage_dir)
        .map_err(|e| format!("Failed to create {}: {}", stage_dir.display(), e))?;
    // Clear every destination first and export parents before children:
    // with nested gits (`p` and `p/q`) replacing `p` would otherwise wipe
    // an already staged `p/q`. The sort is stable, so gits at the same
    // depth keep sdk.yml order.
    for git in gits {
        remove_existing(&stage_dir.join(&git.name))?;
    }
    let mut order: Vec<usize> = (0..gits.len()).collect();
    order.sort_by_key(|&i| Path::new(&gits[i].name).components().count());
    for (git, sha) in order.into_iter().map(|i| (&gits[i], &shas[i])) {
        let dest = stage_dir.join(&git.name);
        fs::create_dir_all(&dest)
            .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
        export_tree(&git.path, sha, &dest)?;
        messages::success(&format!(
            "Staged {} @ {} -> {}",
            git.name,
            &sha[..sha.len().min(12)],
            dest.display()
        ));
    }
    fs::write(&record_path, record)
        .map_err(|e| format!("Failed to write {}: {}", record_path.display(), e))?;
    messages::success(&format!("Wrote {}", record_path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn git(repo: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// A repo with a plain file, an executable, a nested dir and a symlink.
    fn make_repo(root: &Path, name: &str) -> StageGit {
        let path = root.join(name);
        fs::create_dir_all(path.join("sub")).unwrap();
        git(&path, &["init", "-q"]);
        fs::write(path.join("README"), format!("{}\n", name)).unwrap();
        fs::write(path.join("sub/run.sh"), "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path.join("sub/run.sh"), fs::Permissions::from_mode(0o755))
                .unwrap();
            std::os::unix::fs::symlink("sub/run.sh", path.join("link")).unwrap();
        }
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-q", "-m", "init"]);
        StageGit {
            name: name.to_string(),
            url: format!("https://example.com/{}.git", name),
            path,
        }
    }

    fn ws_for(root: &Path, gits: &[StageGit]) -> StageWorkspace {
        StageWorkspace {
            root: root.to_path_buf(),
            checkouts: gits
                .iter()
                .map(|g| (g.name.clone(), g.path.clone()))
                .collect(),
        }
    }

    #[test]
    fn test_stage_exports_head_tree_and_record() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let a = make_repo(&ws, "alpha");
        let b = make_repo(&ws, "beta");
        // Untracked files are fine and never staged.
        fs::write(a.path.join("untracked.txt"), "x").unwrap();

        let out = tmp.path().join("stage");
        fs::create_dir_all(out.join("alpha")).unwrap();
        fs::write(out.join("alpha/stale"), "old").unwrap();
        fs::write(out.join("keep.txt"), "keep").unwrap();

        let gits = [b.clone(), a.clone()];
        stage_gits(&gits, &ws_for(&ws, &gits), &out, false).unwrap();

        assert_eq!(
            fs::read_to_string(out.join("alpha/README")).unwrap(),
            "alpha\n"
        );
        assert_eq!(
            fs::read_to_string(out.join("beta/README")).unwrap(),
            "beta\n"
        );
        assert!(!out.join("alpha/stale").exists(), "old content replaced");
        assert!(!out.join("alpha/untracked.txt").exists());
        assert!(!out.join("alpha/.git").exists());
        assert_eq!(fs::read_to_string(out.join("keep.txt")).unwrap(), "keep");

        // Files and directories carry the commit time, like `tar x`.
        let commit_time: u64 = git(&a.path, &["log", "-1", "--format=%ct"])
            .trim()
            .parse()
            .unwrap();
        let mtime = |p: &str| {
            fs::metadata(out.join(p))
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        };
        assert_eq!(mtime("alpha/README"), commit_time);
        #[cfg(unix)]
        assert_eq!(mtime("alpha/sub"), commit_time);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &str| fs::metadata(out.join(p)).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode("alpha/README"), 0o644);
            assert_eq!(mode("alpha/sub/run.sh"), 0o755);
            let link = out.join("alpha/link");
            assert!(fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read_link(&link).unwrap(), PathBuf::from("sub/run.sh"));
        }

        let sha_a = git(&a.path, &["rev-parse", "HEAD"]).trim().to_string();
        let sha_b = git(&b.path, &["rev-parse", "HEAD"]).trim().to_string();
        let expected = format!(
            "{{\n  \"gits\": {{\n    \"beta\": {{\n      \"sha\": \"{}\",\n      \"url\": \"https://example.com/beta.git\"\n    }},\n    \"alpha\": {{\n      \"sha\": \"{}\",\n      \"url\": \"https://example.com/alpha.git\"\n    }}\n  }}\n}}\n",
            sha_b, sha_a
        );
        assert_eq!(
            fs::read_to_string(out.join(STAGE_RECORD_FILE)).unwrap(),
            expected
        );
    }

    #[test]
    fn test_stage_refuses_tracked_changes_before_writing() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let a = make_repo(&ws, "alpha");
        let b = make_repo(&ws, "beta");
        let c = make_repo(&ws, "gamma");
        fs::write(b.path.join("README"), "changed\n").unwrap();
        fs::write(c.path.join("new"), "staged\n").unwrap();
        git(&c.path, &["add", "new"]);

        let out = tmp.path().join("stage");
        let gits = [a, b, c];
        let err = stage_gits(&gits, &ws_for(&ws, &gits), &out, false).unwrap_err();
        assert!(
            err.contains("uncommitted changes in: beta, gamma"),
            "{}",
            err
        );
        assert!(!out.exists(), "nothing written on refusal");
    }

    #[test]
    fn test_stage_dry_run_writes_nothing() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let a = make_repo(&ws, "alpha");
        let out = tmp.path().join("stage");
        let gits = [a];
        stage_gits(&gits, &ws_for(&ws, &gits), &out, true).unwrap();
        assert!(!out.exists());
    }

    #[test]
    fn test_stage_rejects_non_git_and_overlap() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let a = make_repo(&ws, "alpha");
        let missing = StageGit {
            name: "missing".into(),
            url: "u".into(),
            path: ws.join("missing"),
        };
        let gits = [missing];
        let err =
            stage_gits(&gits, &ws_for(&ws, &gits), &tmp.path().join("stage"), false).unwrap_err();
        assert!(err.contains("is not a git checkout"), "{}", err);

        // Staging into the workspace itself would replace the checkout.
        let gits = [a.clone()];
        let err = stage_gits(&gits, &ws_for(&ws, &gits), &ws, false).unwrap_err();
        assert!(err.contains("overlaps"), "{}", err);
        assert!(a.path.join(".git").exists());
    }

    /// Regression: with `p/q` listed before `p`, staging `p` replaced
    /// `<DIR>/p` and so deleted the already staged `p/q`, while stage.json
    /// still listed it.
    #[test]
    fn test_stage_nested_gits_child_listed_first() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let parent = make_repo(&ws, "p");
        fs::write(parent.path.join(".gitignore"), "/q/\n").unwrap();
        git(&parent.path, &["add", ".gitignore"]);
        git(&parent.path, &["commit", "-q", "-m", "ignore q"]);
        let child = make_repo(&ws, "p/q");
        let gits = [child.clone(), parent.clone()];

        let out = tmp.path().join("stage");
        fs::create_dir_all(out.join("p/q")).unwrap();
        fs::write(out.join("p/q/stale"), "old").unwrap();
        fs::write(out.join("p/stale"), "old").unwrap();

        for _ in 0..2 {
            stage_gits(&gits, &ws_for(&ws, &gits), &out, false).unwrap();
            assert_eq!(fs::read_to_string(out.join("p/README")).unwrap(), "p\n");
            assert_eq!(fs::read_to_string(out.join("p/q/README")).unwrap(), "p/q\n");
            assert!(!out.join("p/stale").exists());
            assert!(!out.join("p/q/stale").exists());
        }
        let record = fs::read_to_string(out.join(STAGE_RECORD_FILE)).unwrap();
        let q = record.find("\"p/q\"").expect("p/q recorded");
        let p = record.find("\"p\"").expect("p recorded");
        assert!(q < p, "record keeps sdk.yml order: {}", record);
    }

    /// Regression: submodules were silently staged as empty directories.
    #[test]
    fn test_stage_warns_about_submodules() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let sub = make_repo(&tmp.path().join("src"), "sub");
        let parent = make_repo(&ws, "p");
        let sha = git(&sub.path, &["rev-parse", "HEAD"]).trim().to_string();
        // Register gitlinks directly, no clone needed.
        for path in ["libs/sub", "q"] {
            git(
                &parent.path,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("160000,{},{}", sha, path),
                ],
            );
        }
        git(&parent.path, &["commit", "-q", "-m", "add submodules"]);
        // As after a clone without --recurse-submodules.
        fs::create_dir_all(parent.path.join("libs/sub")).unwrap();
        fs::create_dir_all(parent.path.join("q")).unwrap();
        let plain = make_repo(&ws, "plain");

        let shas: Vec<String> = [&parent, &plain]
            .iter()
            .map(|g| git(&g.path, &["rev-parse", "HEAD"]).trim().to_string())
            .collect();
        let warnings = submodule_warnings(&[parent.clone(), plain.clone()], &shas).unwrap();
        assert_eq!(
            warnings,
            ["Warning: p has submodules whose content is not staged: libs/sub, q"]
        );

        // p/q staged as a git of its own is not reported.
        let q = StageGit {
            name: "p/q".into(),
            url: "u".into(),
            path: plain.path.clone(),
        };
        let warnings =
            submodule_warnings(&[parent.clone(), q], &[shas[0].clone(), shas[1].clone()]).unwrap();
        assert_eq!(
            warnings,
            ["Warning: p has submodules whose content is not staged: libs/sub"]
        );

        // Staging still succeeds.
        let gits = [parent];
        stage_gits(&gits, &ws_for(&ws, &gits), &tmp.path().join("stage"), false).unwrap();
        assert!(tmp.path().join("stage/p/README").exists());
    }

    #[test]
    fn test_stage_empty_selection_is_an_error() {
        let tmp = TempDir::new().unwrap();
        assert!(stage_gits(&[], &ws_for(tmp.path(), &[]), tmp.path(), false).is_err());
    }

    /// Regression: the overlap check only looked at each selected git's
    /// own checkout, so with gits `a` and `stage/a`, staging only `a` into
    /// `<ws>/stage` replaced (deleted) the unselected checkout `stage/a`.
    #[test]
    fn test_stage_refuses_destination_over_unselected_checkout() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let a = make_repo(&ws, "a");
        let nested = make_repo(&ws, "stage/a");
        let workspace = ws_for(&ws, &[a.clone(), nested.clone()]);

        let err = stage_gits(
            std::slice::from_ref(&a),
            &workspace,
            &ws.join("stage"),
            false,
        )
        .unwrap_err();
        assert!(
            err.contains("overlaps the checkout of git 'stage/a'"),
            "{}",
            err
        );
        assert!(nested.path.join(".git").exists(), "checkout untouched");
        assert_eq!(
            fs::read_to_string(nested.path.join("README")).unwrap(),
            "stage/a\n"
        );

        // A destination inside an unselected checkout is refused too.
        let err =
            stage_gits(std::slice::from_ref(&a), &workspace, &nested.path, false).unwrap_err();
        assert!(err.contains("'stage/a'"), "{}", err);

        // A destination that is, or contains, the workspace root.
        let top = StageGit {
            name: "ws".into(),
            url: "u".into(),
            path: a.path.clone(),
        };
        let err = stage_gits(&[top], &workspace, tmp.path(), false).unwrap_err();
        assert!(err.contains("would replace the workspace"), "{}", err);
        assert!(a.path.join(".git").exists());

        // Elsewhere inside the workspace is fine.
        stage_gits(&[a], &workspace, &ws.join("release"), false).unwrap();
        assert!(ws.join("release/a/README").exists());
    }
}
