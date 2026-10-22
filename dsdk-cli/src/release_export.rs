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

//! `cim release --export-manifest <FILE>`: write a cleaned, standalone
//! sdk.yml for a customer from the workspace's resolved configuration.
//!
//! The export works on YAML data rather than on the typed `SdkConfig`, so
//! key order and keys cim does not model pass through untouched, and
//! variable values stay the raw `${{ VAR }}` template text. Only a target
//! using `extends:` goes through the typed overlay merge, which is what
//! flattens it into one standalone manifest.

use dsdk_cli::config::{self, SdkConfig, SdkConfigCore};
use dsdk_cli::overlay;
use dsdk_cli::workspace::{
    git_name_to_dir_var, load_config_with_extends, OVERLAYS_DIR, SDK_CONFIG_FILE,
};
use noyalib::{Mapping, Value};
use regex::Regex;
use std::fs;
use std::path::Path;

/// Keys that only make sense while resolving `extends:`; the flattened
/// export never contains them.
const EXTENDS_KEYS: [&str; 2] = ["extends", "overlay"];

/// What to change on the way out, as given on the command line.
#[derive(Default)]
pub(crate) struct ExportOptions<'a> {
    /// `{name}` is replaced by the git name to form each git's url.
    pub url_template: Option<&'a str>,
    /// Commit (branch, tag or sha) set on every exported git.
    pub commit: Option<&'a str>,
    /// Phase names removed from `phases:`.
    pub drop_phases: &'a [String],
    /// Variable names matching any of these are removed.
    pub drop_vars: &'a [Regex],
    /// copy_files entries whose `source` matches any of these are removed.
    pub drop_copy_files: &'a [Regex],
}

/// Compile a list of `--drop-*` regexes, naming the option on error.
pub(crate) fn compile_regex_list(patterns: &[String], option: &str) -> Result<Vec<Regex>, String> {
    patterns
        .iter()
        .map(|p| Regex::new(p).map_err(|e| format!("Invalid {} regex '{}': {}", option, p, e)))
        .collect()
}

fn parse_yaml_mapping(path: &Path) -> Result<Mapping, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    let content = config::quote_numeric_commit_values(&content);
    match noyalib::from_str::<Value>(&content)
        .map_err(|e| format!("Cannot parse {}: {}", path.display(), e))?
    {
        Value::Mapping(map) => Ok(map),
        Value::Null => Ok(Mapping::new()),
        _ => Err(format!("{}: top level is not a mapping", path.display())),
    }
}

/// Raw YAML of `config_path` followed by that of every base target it
/// (transitively) extends, nearest first.
fn load_raw_chain(config_path: &Path) -> Result<Vec<Mapping>, String> {
    let overlays_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(OVERLAYS_DIR);
    let mut chain = vec![parse_yaml_mapping(config_path)?];
    let mut path = config_path.to_path_buf();
    loop {
        let level = config::load_config(&path).map_err(|e| e.to_string())?;
        let Some(extends) = level.extends else {
            return Ok(chain);
        };
        path = overlays_dir.join(format!("{}-{}", extends.target, SDK_CONFIG_FILE));
        chain.push(parse_yaml_mapping(&path)?);
    }
}

/// Recursively drop mapping entries whose value is null: the typed config
/// serializes every absent optional field as `null`.
fn strip_nulls(value: &mut Value) {
    match value {
        Value::Mapping(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        Value::Sequence(seq) => seq.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

/// Keys of `maps` in order of first appearance.
fn ordered_keys<'a>(maps: impl IntoIterator<Item = &'a Mapping>) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for map in maps {
        for key in map.keys() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    keys
}

/// Reorder `map` by `order`; keys not in `order` keep their relative order
/// at the end.
fn reorder(map: Mapping, order: &[String]) -> Mapping {
    let mut map = map;
    let mut out = Mapping::new();
    for key in order {
        if let Some(v) = map.shift_remove(key) {
            out.insert(key.clone(), v);
        }
    }
    out.extend(map);
    out
}

/// The resolved manifest as YAML data. A plain target is its sdk.yml as
/// written. A target using `extends:` is flattened through the same
/// overlay merge cim uses everywhere else; top-level keys keep the order
/// of the target's own sdk.yml (then the bases'), variables the order in
/// which the chain declares them (bases first), and top-level keys cim
/// does not model are taken from the nearest file declaring them.
pub(crate) fn resolved_manifest(config_path: &Path) -> Result<Mapping, String> {
    let chain = load_raw_chain(config_path)?;
    if chain.len() == 1 {
        return Ok(chain.into_iter().next().unwrap_or_default());
    }

    let merged = load_config_with_extends(config_path).map_err(|e| e.to_string())?;
    let Value::Mapping(typed) =
        noyalib::to_value(&merged).map_err(|e| format!("Cannot serialize config: {}", e))?
    else {
        return Err("Resolved config did not serialize to a mapping".to_string());
    };

    let mut out = Mapping::new();
    for key in ordered_keys(&chain) {
        if EXTENDS_KEYS.contains(&key.as_str()) {
            continue;
        }
        match typed.get(&key) {
            Some(v) if v.is_null() => {}
            Some(v) => {
                out.insert(key.clone(), v.clone());
            }
            None => {
                if let Some(v) = chain.iter().find_map(|m| m.get(&key)) {
                    out.insert(key.clone(), v.clone());
                }
            }
        }
    }
    for (key, v) in typed {
        if !out.contains_key(&key) && !v.is_null() && !EXTENDS_KEYS.contains(&key.as_str()) {
            out.insert(key, v);
        }
    }

    let var_order = ordered_keys(
        chain
            .iter()
            .rev()
            .filter_map(|m| m.get("variables").and_then(Value::as_mapping)),
    );
    if let Some(Value::Mapping(vars)) = out.get_mut("variables") {
        *vars = reorder(std::mem::take(vars), &var_order);
    }

    let mut value = Value::Mapping(out);
    strip_nulls(&mut value);
    match value {
        Value::Mapping(map) => Ok(map),
        _ => unreachable!("mapping stays a mapping"),
    }
}

fn git_name(entry: &Value) -> Option<&str> {
    entry.as_mapping()?.get("name")?.as_str()
}

/// Name of a `phases:` entry: either the plain string or the `name:` of
/// the mapping form.
fn phase_name(entry: &Value) -> Option<&str> {
    match entry {
        Value::String(name) => Some(name),
        Value::Mapping(map) => map.get("name").and_then(Value::as_str),
        _ => None,
    }
}

/// Refuse `--drop-phase` names that would silently do nothing: a typo
/// would let an internal phase ship. Standard phases are not in
/// `phases:` and cannot be dropped this way.
fn check_drop_phase_names(doc: &Mapping, drop: &[String]) -> Result<(), String> {
    let known: Vec<&str> = match doc.get("phases") {
        Some(Value::Sequence(phases)) => phases.iter().filter_map(phase_name).collect(),
        _ => Vec::new(),
    };
    let standard = config::default_phases();
    let mut problems = Vec::new();
    for name in drop {
        if standard.contains(name) {
            problems.push(format!(
                "'{}' is a standard phase; --drop-phase only removes custom phases \
                 listed in phases:. Edit or remove the top-level '{}:' block in sdk.yml \
                 instead",
                name, name
            ));
        } else if !known.contains(&name.as_str()) {
            problems.push(format!(
                "'{}' matches no phase in phases: (phases: {})",
                name,
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("--drop-phase: {}", problems.join("; ")))
    }
}

fn typed_config(doc: &Mapping) -> Result<SdkConfig, String> {
    let yaml = noyalib::to_string(&Value::Mapping(doc.clone()))
        .map_err(|e| format!("Cannot serialize manifest: {}", e))?;
    noyalib::from_str(&config::quote_numeric_commit_values(&yaml))
        .map_err(|e| format!("Exported manifest does not load: {}", e))
}

/// One dependency reference in a manifest: where it is, what kind of
/// thing it may name, and the name.
struct DepRef {
    location: String,
    /// Only git names resolve (git_depends_on, depends_on_gits).
    git_only: bool,
    /// A bare phase name resolves too (custom-phase depends_on).
    bare_phase: bool,
    name: String,
}

fn dep_refs(cfg: &SdkConfig) -> Vec<DepRef> {
    let mut refs = Vec::new();
    let mut push = |location: String, git_only: bool, bare_phase: bool, deps: &[String]| {
        for name in deps {
            refs.push(DepRef {
                location: location.clone(),
                git_only,
                bare_phase,
                name: name.clone(),
            });
        }
    };
    for git in &cfg.gits {
        if let Some(deps) = &git.build_depends_on {
            push(
                format!("gits[{}].build_depends_on", git.name),
                false,
                false,
                deps,
            );
        }
        if let Some(deps) = &git.git_depends_on {
            push(
                format!("gits[{}].git_depends_on", git.name),
                true,
                false,
                deps,
            );
        }
    }
    for install in cfg.install.iter().flatten() {
        if let Some(deps) = &install.depends_on_gits {
            push(
                format!("install[{}].depends_on_gits", install.name),
                true,
                false,
                deps,
            );
        }
    }
    for phase in config::default_phases() {
        if let Some(deps) = cfg.phase_target(&phase).and_then(|t| t.depends_on()) {
            push(format!("{}.depends_on", phase), false, false, deps);
        }
    }
    for entry in cfg.phases.iter().flatten() {
        push(
            format!("phases[{}].depends_on", entry.name),
            false,
            true,
            entry.depends_on(),
        );
    }
    refs
}

/// What `name` stands for in `cfg`, if anything: a git, a phase
/// (`sdk-<phase>`, or the bare name where allowed), an install target or
/// a `<git>-<phase>` fragment target.
fn resolve_ref(cfg: &SdkConfig, r: &DepRef) -> Option<String> {
    let name = r.name.as_str();
    if cfg.gits.iter().any(|g| g.name == name) {
        return Some(format!("git '{}'", name));
    }
    if r.git_only {
        return None;
    }
    let phases = cfg.phases();
    let phase = name
        .strip_prefix("sdk-")
        .or(r.bare_phase.then_some(name))
        .filter(|p| phases.iter().any(|known| known == p));
    if let Some(p) = phase {
        return Some(format!("phase '{}'", p));
    }
    if let Some(i) = name.strip_prefix("install-") {
        if cfg.install.iter().flatten().any(|inst| inst.name == i) {
            return Some(format!("install '{}'", i));
        }
    }
    cfg.gits.iter().find_map(|g| {
        let base = g.name.rsplit('/').next().unwrap_or(&g.name);
        let p = name.strip_prefix(base)?.strip_prefix('-')?;
        phases
            .iter()
            .any(|known| known == p)
            .then(|| format!("target of git '{}'", g.name))
    })
}

/// `${{ NAME }}` and `$(NAME)` references in `s`.
fn var_refs(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("${{") {
        let after = &rest[i + 3..];
        let Some(end) = after.find("}}") else { break };
        out.push(after[..end].trim());
        rest = &after[end + 2..];
    }
    let mut rest = s;
    while let Some(i) = rest.find("$(") {
        let after = &rest[i + 2..];
        let Some(end) = after.find(')') else { break };
        out.push(after[..end].trim());
        rest = &after[end + 1..];
    }
    out
}

/// Collect `(location, variable)` for every reference in `value` to one
/// of `gone`.
fn find_var_refs(value: &Value, path: &str, gone: &[String], out: &mut Vec<(String, String)>) {
    match value {
        Value::String(s) => {
            for name in var_refs(s) {
                if gone.iter().any(|g| g == name) {
                    out.push((path.to_string(), name.to_string()));
                }
            }
        }
        Value::Mapping(map) => {
            for (k, v) in map {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                find_var_refs(v, &sub, gone, out);
            }
        }
        Value::Sequence(seq) => {
            for (i, v) in seq.iter().enumerate() {
                let sub = match v
                    .as_mapping()
                    .and_then(|m| m.get("name"))
                    .and_then(Value::as_str)
                {
                    Some(name) => format!("{}[{}]", path, name),
                    None => format!("{}[{}]", path, i),
                };
                find_var_refs(v, &sub, gone, out);
            }
        }
        _ => {}
    }
}

/// Variables the export removed that the output may still reference:
/// dropped `variables:` and the `<GIT>_DIR` of gits not exported, unless
/// the output still defines that name.
fn removed_vars(input: &SdkConfig, output: &SdkConfig) -> Vec<String> {
    let defined = |cfg: &SdkConfig| -> Vec<String> {
        let mut names: Vec<String> = cfg
            .variables
            .iter()
            .flat_map(|v| v.keys().cloned())
            .collect();
        names.extend(cfg.gits.iter().map(|g| git_name_to_dir_var(&g.name)));
        names
    };
    let kept = defined(output);
    let mut gone: Vec<String> = defined(input)
        .into_iter()
        .filter(|n| !kept.contains(n))
        .collect();
    gone.sort();
    gone.dedup();
    gone
}

/// Refuse an export that would not work on its own: every dependency
/// reference that resolved in the input but no longer does (a git that
/// is not exported, a dropped phase), every reference to a variable that
/// was removed (it would silently fall back to the host environment),
/// and anything `cim init`/`cim makefile` would reject when loading the
/// exported manifest. Nothing is rewritten; each problem is listed with
/// its location.
fn validate_export(input_doc: &Mapping, output_doc: &Mapping) -> Result<(), String> {
    let input = typed_config(input_doc)?;
    let output = typed_config(output_doc)?;

    let mut problems = Vec::new();
    for r in dep_refs(&output) {
        if resolve_ref(&output, &r).is_some() {
            continue;
        }
        if let Some(what) = resolve_ref(&input, &r) {
            problems.push(format!(
                "{}: '{}' names {}, which is not exported",
                r.location, r.name, what
            ));
        }
    }

    let gone = removed_vars(&input, &output);
    let mut refs = Vec::new();
    find_var_refs(&Value::Mapping(output_doc.clone()), "", &gone, &mut refs);
    for (location, name) in refs {
        problems.push(format!(
            "{}: references variable '{}', which is not exported",
            location, name
        ));
    }

    if problems.is_empty() {
        if let Err(e) = overlay::validate_dependencies(&output) {
            problems.push(e);
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the exported manifest would have dangling references:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

/// Apply the export rules to a resolved manifest. Returns the number of
/// gits exported.
pub(crate) fn clean_manifest(
    doc: &mut Mapping,
    selected: &dyn Fn(&str) -> bool,
    opts: &ExportOptions,
) -> Result<usize, String> {
    for key in EXTENDS_KEYS {
        doc.shift_remove(key);
    }
    check_drop_phase_names(doc, opts.drop_phases)?;
    let input = doc.clone();

    let mut exported = 0;
    if let Some(Value::Sequence(gits)) = doc.get_mut("gits") {
        gits.retain(|g| git_name(g).is_some_and(selected));
        for git in gits.iter_mut() {
            let name = git_name(git).unwrap_or_default().to_string();
            let Value::Mapping(map) = git else { continue };
            map.shift_remove("group");
            if let Some(template) = opts.url_template {
                map.insert("url", Value::from(template.replace("{name}", &name)));
            }
            if let Some(commit) = opts.commit {
                map.insert("commit", Value::from(commit));
            }
        }
        exported = gits.len();
    }
    if exported == 0 {
        return Err("No gits selected for the exported manifest".to_string());
    }

    if !opts.drop_phases.is_empty() {
        if let Some(Value::Sequence(phases)) = doc.get_mut("phases") {
            let dropped = |p: &Value| {
                phase_name(p).is_some_and(|n| opts.drop_phases.contains(&n.to_string()))
            };
            phases.retain(|p| !dropped(p));
            if phases.is_empty() {
                doc.shift_remove("phases");
            }
        }
    }

    if !opts.drop_vars.is_empty() {
        if let Some(Value::Mapping(vars)) = doc.get_mut("variables") {
            vars.retain(|k, _| !opts.drop_vars.iter().any(|r| r.is_match(k)));
        }
    }

    if !opts.drop_copy_files.is_empty() {
        if let Some(Value::Sequence(files)) = doc.get_mut("copy_files") {
            files.retain(|f| {
                let source = f
                    .as_mapping()
                    .and_then(|m| m.get("source"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                !opts.drop_copy_files.iter().any(|r| r.is_match(source))
            });
            if files.is_empty() {
                doc.shift_remove("copy_files");
            }
        }
    }

    validate_export(&input, doc)?;
    Ok(exported)
}

/// Build the exported manifest text for the workspace sdk.yml at
/// `config_path`. Returns the YAML and the number of exported gits.
pub(crate) fn export_manifest(
    config_path: &Path,
    selected: &dyn Fn(&str) -> bool,
    opts: &ExportOptions,
) -> Result<(String, usize), String> {
    let mut doc = resolved_manifest(config_path)?;
    let count = clean_manifest(&mut doc, selected, opts)?;
    let mut yaml = noyalib::to_string(&Value::Mapping(doc))
        .map_err(|e| format!("Cannot serialize manifest: {}", e))?;
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    Ok((yaml, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    fn all(_: &str) -> bool {
        true
    }

    fn export(path: &Path, selected: &dyn Fn(&str) -> bool, opts: &ExportOptions) -> Value {
        let (yaml, _) = export_manifest(path, selected, opts).unwrap();
        noyalib::from_str(&yaml).unwrap()
    }

    fn parse(yaml: &str) -> Value {
        noyalib::from_str(yaml).unwrap()
    }

    const DEMO: &str = r#"
# internal comment
phases: [prune, release-verify, release-deliver]

variables:
  CUSTOMER: none
  RELEASE_DIR: ${{ WORKSPACE }}/release/$(CUSTOMER)
  DELIVERY_ROOT: ${{ HOME }}/devel/customer-releases
  OUTPUT_DIR: ${{ WORKSPACE }}/out
  PYTHON: ${{ WORKSPACE }}/.venv/bin/python3

gits:
  - name: build
    url: file:///src/build
    commit: prune
  - name: drivers
    url: file:///src/drivers
    commit: main
    build_depends_on: [build]
  - name: release-tools
    url: file:///src/release-tools
    commit: main
    group: [release]

copy_files:
  - source: profiles/acme.yml
    dest: profiles/acme.yml
  - source: profiles/globex.yml
    dest: profiles/globex.yml
"#;

    #[test]
    fn test_export_demo_like_release() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, DEMO);
        let drop_vars = compile_regex_list(
            &["^(CUSTOMER|DELIVERY_ROOT|RELEASE_.*)$".to_string()],
            "--drop-var",
        )
        .unwrap();
        let drop_files = compile_regex_list(&["^profiles/".to_string()], "x").unwrap();
        let phases: Vec<String> = ["prune", "release-verify", "release-deliver"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let opts = ExportOptions {
            url_template: Some("file:///D/acme/{name}.git"),
            commit: Some("v2026.10.0"),
            drop_phases: &phases,
            drop_vars: &drop_vars,
            drop_copy_files: &drop_files,
        };
        let (yaml, count) =
            export_manifest(&path, &|n| n == "build" || n == "drivers", &opts).unwrap();
        assert_eq!(count, 2);
        assert!(!yaml.contains('#'), "no comments: {}", yaml);
        assert!(yaml.ends_with('\n'));
        assert_eq!(
            parse(&yaml),
            parse(
                r#"
variables:
  OUTPUT_DIR: "${{ WORKSPACE }}/out"
  PYTHON: "${{ WORKSPACE }}/.venv/bin/python3"
gits:
  - {name: build, url: "file:///D/acme/build.git", commit: v2026.10.0}
  - {name: drivers, url: "file:///D/acme/drivers.git", commit: v2026.10.0, build_depends_on: [build]}
"#
            )
        );
        // Top-level and variable key order as in the input.
        let doc = parse(&yaml);
        let keys: Vec<_> = doc.as_mapping().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["variables", "gits"]);
        let vars: Vec<_> = doc["variables"]
            .as_mapping()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(vars, ["OUTPUT_DIR", "PYTHON"]);
    }

    #[test]
    fn test_export_without_options_passes_everything_through() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "custom_key: {a: 1}\nvariables: {B: '${{ A }}/b', A: x}\ngits:\n  - name: g\n    url: u\n    commit: 0123\n    group: g1\n    future_key: kept\nphases: [deploy]\n",
        );
        let doc = export(&path, &all, &ExportOptions::default());
        assert_eq!(
            doc,
            parse("custom_key: {a: 1}\nvariables: {B: '${{ A }}/b', A: x}\ngits:\n  - {name: g, url: u, commit: '0123', future_key: kept}\nphases: [deploy]\n")
        );
        let keys: Vec<_> = doc.as_mapping().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["custom_key", "variables", "gits", "phases"]);
    }

    #[test]
    fn test_export_git_selection_url_and_commit() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, DEMO);
        let opts = ExportOptions {
            url_template: Some("https://x/{name}/{name}.git"),
            ..Default::default()
        };
        let doc = export(&path, &|n| n.starts_with("release"), &opts);
        assert_eq!(
            doc["gits"],
            parse("- {name: release-tools, url: 'https://x/release-tools/release-tools.git', commit: main}")
        );

        let opts = ExportOptions {
            commit: Some("v1"),
            ..Default::default()
        };
        let doc = export(&path, &|n| n == "build", &opts);
        assert_eq!(
            doc["gits"],
            parse("- {name: build, url: 'file:///src/build', commit: v1}")
        );

        assert!(export_manifest(&path, &|_| false, &opts).is_err());
    }

    #[test]
    fn test_export_drop_phase() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, DEMO);
        let some = vec!["prune".to_string()];
        let opts = ExportOptions {
            drop_phases: &some,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert_eq!(doc["phases"], parse("[release-verify, release-deliver]"));

        let every: Vec<String> = ["prune", "release-verify", "release-deliver"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let opts = ExportOptions {
            drop_phases: &every,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert!(doc.as_mapping().unwrap().get("phases").is_none());
    }

    const MAPPING_PHASES: &str = r#"
phases:
  - prune
  - name: release-verify
    depends_on: [prune]
  - name: release-deliver
    depends_on: [release-verify]
    commands: [echo deliver]
  - docs
gits:
  - name: build
    url: u
    commit: main
"#;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_export_drop_phase_mapping_entries() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, MAPPING_PHASES);
        let drop = names(&["prune", "release-verify", "release-deliver"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert_eq!(doc["phases"], parse("[docs]"));

        let drop = names(&["prune", "release-verify", "release-deliver", "docs"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert!(doc.as_mapping().unwrap().get("phases").is_none());

        // Dropping only the leaf keeps the mapping entries it doesn't touch.
        let drop = names(&["release-deliver"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert_eq!(
            doc["phases"],
            parse("[prune, {name: release-verify, depends_on: [prune]}, docs]")
        );
    }

    #[test]
    fn test_export_drop_phase_refuses_dangling_depends_on() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, MAPPING_PHASES);
        let drop = names(&["prune"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(
            err.contains(
                "phases[release-verify].depends_on: 'prune' names phase 'prune', which is not exported"
            ),
            "{}",
            err
        );

        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "phases:\n  - prune\n  - name: docs\n    depends_on: [sdk-prune]\ngits:\n  - {name: build, url: u, commit: main}\n",
        );
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(
            err.contains(
                "phases[docs].depends_on: 'sdk-prune' names phase 'prune', which is not exported"
            ),
            "{}",
            err
        );
    }

    #[test]
    fn test_export_drop_var_keeps_raw_templates() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, DEMO);
        let drop_vars =
            compile_regex_list(&["^RELEASE_".to_string(), "^CUSTOMER$".to_string()], "x").unwrap();
        let opts = ExportOptions {
            drop_vars: &drop_vars,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert_eq!(
            doc["variables"],
            parse(
                "DELIVERY_ROOT: '${{ HOME }}/devel/customer-releases'\nOUTPUT_DIR: '${{ WORKSPACE }}/out'\nPYTHON: '${{ WORKSPACE }}/.venv/bin/python3'"
            )
        );
        assert!(compile_regex_list(&["(".to_string()], "--drop-var").is_err());
    }

    #[test]
    fn test_export_drop_copy_files() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, DEMO);
        let one = compile_regex_list(&["acme".to_string()], "x").unwrap();
        let opts = ExportOptions {
            drop_copy_files: &one,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert_eq!(
            doc["copy_files"],
            parse("- {source: profiles/globex.yml, dest: profiles/globex.yml}")
        );

        let every = compile_regex_list(&["^profiles/".to_string()], "x").unwrap();
        let opts = ExportOptions {
            drop_copy_files: &every,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert!(doc.as_mapping().unwrap().get("copy_files").is_none());
    }

    #[test]
    fn test_export_keeps_min_cim_version() {
        let tmp = TempDir::new().unwrap();
        // Plain target: passed through as written.
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "min_cim_version: \"1.0.0\"\ngits:\n  - {name: g, url: u, commit: main}\n",
        );
        let doc = export(&path, &all, &ExportOptions::default());
        assert_eq!(doc["min_cim_version"], parse("'1.0.0'"));

        // Flattened extends chain: the highest requirement of all levels.
        write(
            tmp.path(),
            &format!("{}/base-{}", OVERLAYS_DIR, SDK_CONFIG_FILE),
            "min_cim_version: \"1.1.0\"\ngits:\n  - {name: core, url: u, commit: main}\n",
        );
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "min_cim_version: \"1.0.0\"\nextends: base\ngits:\n  - {name: app, url: u, commit: main}\n",
        );
        let doc = export(&path, &all, &ExportOptions::default());
        assert_eq!(doc["min_cim_version"], parse("'1.1.0'"));
        let keys: Vec<_> = doc.as_mapping().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["min_cim_version", "gits"]);

        // Only the base declares it.
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "extends: base\ngits:\n  - {name: app, url: u, commit: main}\n",
        );
        let doc = export(&path, &all, &ExportOptions::default());
        assert_eq!(doc["min_cim_version"], parse("'1.1.0'"));
    }

    #[test]
    fn test_export_flattens_extends_and_overlay() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            &format!("{}/base-{}", OVERLAYS_DIR, SDK_CONFIG_FILE),
            r#"
variables:
  OUT: ${{ WORKSPACE }}/out
  GONE: x
gits:
  - name: core
    url: https://example.com/core.git
    commit: main
    group: base
  - name: extra
    url: https://example.com/extra.git
    commit: main
build:
  - make -C core
"#,
        );
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            r#"
extends: base@v1
phases: [deploy]
variables:
  APP: ${{ OUT }}/app
gits:
  - name: app
    url: https://example.com/app.git
    commit: dev
    build_depends_on: [core]
overlay:
  gits:
    remove: [extra]
    modify:
      - name: core
        commit: release-branch
  variables:
    remove: [GONE]
"#,
        );

        let doc = export(&path, &all, &ExportOptions::default());
        assert_eq!(
            doc,
            parse(
                r#"
phases: [deploy]
variables:
  OUT: "${{ WORKSPACE }}/out"
  APP: "${{ OUT }}/app"
gits:
  - {name: core, url: "https://example.com/core.git", commit: release-branch}
  - {name: app, url: "https://example.com/app.git", commit: dev, build_depends_on: [core]}
build: [make -C core]
"#
            )
        );
        let keys: Vec<_> = doc.as_mapping().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["phases", "variables", "gits", "build"]);
        let vars: Vec<_> = doc["variables"]
            .as_mapping()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(vars, ["OUT", "APP"]);
    }

    /// Regression: a --drop-phase name matching nothing (a typo) was
    /// silently ignored, so the internal phase shipped.
    #[test]
    fn test_export_drop_phase_unknown_or_standard_name() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, DEMO);
        let drop = names(&["prnue"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(
            err.contains("'prnue' matches no phase in phases: (phases: prune, release-verify, release-deliver)"),
            "{}",
            err
        );

        let drop = names(&["prune", "build"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(err.contains("'build' is a standard phase"), "{}", err);
        assert!(err.contains("top-level 'build:' block"), "{}", err);

        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "gits:\n  - {name: g, url: u, commit: main}\n",
        );
        let drop = names(&["prune"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(err.contains("(phases: none)"), "{}", err);
    }

    /// Regression: references to gits that are not exported, or to dropped
    /// phases, survived into the exported manifest.
    #[test]
    fn test_export_refuses_dangling_dependency_references() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            r#"
phases:
  - prune
  - name: verify
    depends_on: [tools, install-tool]
gits:
  - name: core
    url: u
    commit: main
  - name: drivers
    url: u
    commit: main
    build_depends_on: [core, tools, tools-build, sdk-prune]
    git_depends_on: [core, tools]
  - name: tools
    url: u
    commit: main
install:
  - name: tool
    depends_on_gits: [tools]
    commands: [echo]
build:
  depends_on: [sdk-prune, tools, install-tool, some-make-target]
  commands: ["@echo build"]
test:
  depends_on: [core]
  commands: ["@echo test"]
"#,
        );
        let drop = names(&["prune"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &|n| n != "tools", &opts).unwrap_err();
        for expected in [
            "gits[drivers].build_depends_on: 'tools' names git 'tools', which is not exported",
            "gits[drivers].build_depends_on: 'tools-build' names target of git 'tools', which is not exported",
            "gits[drivers].build_depends_on: 'sdk-prune' names phase 'prune', which is not exported",
            "gits[drivers].git_depends_on: 'tools' names git 'tools', which is not exported",
            "install[tool].depends_on_gits: 'tools' names git 'tools', which is not exported",
            "build.depends_on: 'sdk-prune' names phase 'prune', which is not exported",
            "build.depends_on: 'tools' names git 'tools', which is not exported",
            "phases[verify].depends_on: 'tools' names git 'tools', which is not exported",
        ] {
            assert!(err.contains(expected), "missing '{}' in:\n{}", expected, err);
        }
        // What still resolves, or never named anything cim knows, is not
        // reported.
        assert!(!err.contains("'core'"), "{}", err);
        assert!(!err.contains("install-tool"), "{}", err);
        assert!(!err.contains("some-make-target"), "{}", err);
        assert_eq!(err.matches("\n  - ").count(), 8, "{}", err);
    }

    /// Regression: --drop-var left `${{ VAR }}` / `$(VAR)` references that
    /// would silently fall back to the host environment.
    #[test]
    fn test_export_refuses_references_to_removed_variables() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            r#"
variables:
  CUSTOMER: acme
  OUT: ${{ WORKSPACE }}/out/${{ CUSTOMER }}
  KEEP: x
gits:
  - name: core
    url: u
    commit: main
    build: ["make -C core OUT=$(OUT) C=$(CUSTOMER) T=${{ RELEASE_TOOLS_DIR }}"]
  - name: release-tools
    url: u
    commit: main
build:
  commands: ["echo ${{CUSTOMER}} $(MAKE) $(KEEP) ${{ CORE_DIR }}"]
"#,
        );
        let drop = compile_regex_list(&["^CUSTOMER$".to_string()], "x").unwrap();
        let opts = ExportOptions {
            drop_vars: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &|n| n == "core", &opts).unwrap_err();
        for expected in [
            "variables.OUT: references variable 'CUSTOMER', which is not exported",
            "gits[core].build[0]: references variable 'CUSTOMER', which is not exported",
            "gits[core].build[0]: references variable 'RELEASE_TOOLS_DIR', which is not exported",
            "build.commands[0]: references variable 'CUSTOMER', which is not exported",
        ] {
            assert!(
                err.contains(expected),
                "missing '{}' in:\n{}",
                expected,
                err
            );
        }
        assert_eq!(err.matches("\n  - ").count(), 4, "{}", err);

        // Dropping what references it too is fine.
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "variables:\n  CUSTOMER: acme\n  OUT: ${{ CUSTOMER }}/out\n  KEEP: $(OTHER)\ngits:\n  - {name: core, url: u, commit: main}\n",
        );
        let drop = compile_regex_list(&["^(CUSTOMER|OUT)$".to_string()], "x").unwrap();
        let opts = ExportOptions {
            drop_vars: &drop,
            ..Default::default()
        };
        let doc = export(&path, &all, &opts);
        assert_eq!(doc["variables"], parse("KEEP: $(OTHER)"));
    }

    #[test]
    fn test_export_passes_unknown_top_level_keys() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "min_cim_version: '0.9.0'\nfuture: {x: [1, 2]}\ngits:\n  - {name: g, url: u, commit: main}\n",
        );
        let doc = export(&path, &all, &ExportOptions::default());
        assert_eq!(
            doc,
            parse("min_cim_version: '0.9.0'\nfuture: {x: [1, 2]}\ngits:\n  - {name: g, url: u, commit: main}\n")
        );
    }

    /// Load an exported manifest the way `cim makefile` does (typed load,
    /// phase and dependency validation, Makefile generation) and check that
    /// every prerequisite in the generated Makefile is a defined target.
    fn assert_loads_and_generates(yaml: &str) {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, yaml);
        let cfg = config::load_config(&path).unwrap();
        config::validate_phases(&cfg).unwrap();
        overlay::validate_dependencies(&cfg).unwrap();
        let makefile = crate::makefile::generate_makefile_content(&cfg, false, None);

        // A git without build: has no rule; its checkout directory is
        // what satisfies the prerequisite in a workspace.
        let mut defined: std::collections::HashSet<String> =
            cfg.gits.iter().map(|g| g.name.clone()).collect();
        let mut prereqs = Vec::new();
        for line in makefile.lines() {
            if line.is_empty() || line.starts_with(['\t', '#', ' ']) {
                continue;
            }
            let Some((targets, deps)) = line.split_once(':') else {
                continue;
            };
            if deps.starts_with('=') || targets.contains('=') || targets.starts_with('.') {
                continue;
            }
            defined.extend(targets.split_whitespace().map(str::to_string));
            prereqs.extend(
                deps.split(';')
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .filter(|d| *d != "|" && !d.starts_with('$'))
                    .map(|d| (targets.to_string(), d.to_string())),
            );
        }
        for (target, dep) in prereqs {
            assert!(
                defined.contains(&dep),
                "'{}' depends on undefined '{}' in:\n{}",
                target,
                dep,
                makefile
            );
        }
    }

    const C2_C4: &str = r#"
phases:
  - prune
  - name: release-verify
    depends_on: [prune, release-tools]
  - name: lint
    commands: ["@echo lint"]
gits:
  - name: drivers
    url: file:///src/drivers
    commit: main
    build: ["make -C drivers"]
  - name: release-tools
    url: file:///src/release-tools
    commit: main
    # A phase may only depend on a git that has a Make target (C2).
    build: ["@true"]
build:
  depends_on: [sdk-prune]
  commands: ["@echo build"]
"#;

    /// Mapping-form custom phases (C2) through --export-manifest (C4).
    #[test]
    fn test_export_mapping_phases_end_to_end() {
        let tmp = TempDir::new().unwrap();
        let path = write(tmp.path(), SDK_CONFIG_FILE, C2_C4);
        // The input itself is a valid workspace manifest.
        assert_loads_and_generates(C2_C4);
        // The checker itself catches a dangling prerequisite.
        assert!(std::panic::catch_unwind(|| assert_loads_and_generates(
            "gits: [{name: g, url: u, commit: main}]\nbuild: {depends_on: [sdk-nope], commands: [x]}\n"
        ))
        .is_err());

        // (a) Dropping prune alone leaves release-verify and build pointing
        // at it.
        let drop = names(&["prune"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(
            err.contains("phases[release-verify].depends_on: 'prune' names phase 'prune'"),
            "{}",
            err
        );
        assert!(
            err.contains("build.depends_on: 'sdk-prune' names phase 'prune'"),
            "{}",
            err
        );

        // (b) Dropping release-verify and release-tools too still leaves
        // build -> sdk-prune.
        let drop = names(&["prune", "release-verify"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let no_tools = |n: &str| n != "release-tools";
        let err = export_manifest(&path, &no_tools, &opts).unwrap_err();
        assert!(
            err.contains("build.depends_on: 'sdk-prune' names phase 'prune'"),
            "{}",
            err
        );
        assert!(!err.contains("release-verify"), "{}", err);
        assert!(!err.contains("release-tools"), "{}", err);

        // ... and succeeds once build no longer depends on it, keeping lint
        // in mapping form with its commands.
        let fixed = C2_C4.replace("  depends_on: [sdk-prune]\n", "");
        let path = write(tmp.path(), SDK_CONFIG_FILE, &fixed);
        let (yaml, count) = export_manifest(&path, &no_tools, &opts).unwrap();
        assert_eq!(count, 1);
        let doc = parse(&yaml);
        assert_eq!(
            doc["phases"],
            parse("[{name: lint, commands: ['@echo lint']}]")
        );
        assert_eq!(
            doc["gits"],
            parse("- {name: drivers, url: 'file:///src/drivers', commit: main, build: [make -C drivers]}")
        );
        // (c) What was exported loads back and generates a Makefile with
        // no dangling prerequisite.
        assert_loads_and_generates(&yaml);
        // Exporting everything is fine as well.
        let (yaml, _) = export_manifest(&path, &all, &ExportOptions::default()).unwrap();
        assert_loads_and_generates(&yaml);
    }

    /// (c) via extends:: the mapping phases come from the base target.
    #[test]
    fn test_export_mapping_phases_from_extends_base() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            &format!("{}/base-{}", OVERLAYS_DIR, SDK_CONFIG_FILE),
            &C2_C4.replace("  depends_on: [sdk-prune]\n", ""),
        );
        let path = write(
            tmp.path(),
            SDK_CONFIG_FILE,
            "extends: base@v1\ngits:\n  - name: app\n    url: u\n    commit: main\n    build_depends_on: [drivers]\n",
        );

        let drop = names(&["prune"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let err = export_manifest(&path, &all, &opts).unwrap_err();
        assert!(
            err.contains("phases[release-verify].depends_on: 'prune'"),
            "{}",
            err
        );

        let drop = names(&["prune", "release-verify"]);
        let opts = ExportOptions {
            drop_phases: &drop,
            ..Default::default()
        };
        let (yaml, count) = export_manifest(&path, &|n: &str| n != "release-tools", &opts).unwrap();
        assert_eq!(count, 2);
        let doc = parse(&yaml);
        assert!(doc.as_mapping().unwrap().get("extends").is_none());
        assert_eq!(
            doc["phases"],
            parse("[{name: lint, commands: ['@echo lint']}]")
        );
        assert_loads_and_generates(&yaml);

        // Excluding a git another one builds on is refused.
        let err = export_manifest(&path, &|n: &str| n == "app", &opts).unwrap_err();
        assert!(
            err.contains("gits[app].build_depends_on: 'drivers' names git 'drivers'"),
            "{}",
            err
        );
    }
}
