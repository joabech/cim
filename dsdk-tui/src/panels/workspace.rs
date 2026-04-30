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

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Paragraph,
    Frame,
};

use crate::cim::{self, CimCommand};
use crate::components::{Checkbox, DropdownSelect, TextInput};
use crate::panels::{CommandPanel, PanelAction};
use crate::theme::theme;

// --- InitPanel ---

#[derive(PartialEq)]
enum InitFocus {
    Target,
    Source,
    Version,
    Workspace,
    Match,
    Toggles,
}

pub struct InitPanel {
    focus: InitFocus,
    target: DropdownSelect,
    source: TextInput,
    version: DropdownSelect,
    workspace: TextInput,
    match_pattern: TextInput,
    no_mirror: bool,
    force: bool,
    verbose: bool,
    install: bool,
    full: bool,
    no_sudo: bool,
    symlink: bool,
    yes: bool,
    cert_validation: DropdownSelect,
    targets_loaded: bool,
    target_rx: Option<tokio::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    version_rx: Option<tokio::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
}

impl InitPanel {
    pub fn new() -> Self {
        Self {
            focus: InitFocus::Target,
            target: DropdownSelect::new("Target", vec![]),
            source: TextInput::new("Source"),
            version: DropdownSelect::new("Version", vec![]),
            workspace: TextInput::new("Workspace"),
            match_pattern: TextInput::new("Match"),
            no_mirror: false,
            force: false,
            verbose: false,
            install: false,
            full: false,
            no_sudo: false,
            symlink: false,
            yes: false,
            cert_validation: DropdownSelect::new(
                "Cert validation",
                vec!["default".to_string(), "os".to_string(), "none".to_string()],
            ),
            targets_loaded: false,
            target_rx: None,
            version_rx: None,
        }
    }

    fn load_targets(&mut self) {
        let source = self.source.value.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        self.target_rx = Some(rx);
        self.target.is_loading = true;
        self.target.loading_text = Some("Loading targets...".to_string());
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || cim::fetch_targets(&source)).await;
            match result {
                Ok(Ok(targets)) => {
                    let _ = tx.send(Ok(targets)).await;
                }
                Ok(Err(e)) => {
                    let _ = tx.send(Err(e.to_string())).await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string())).await;
                }
            }
        });
    }

    fn load_versions(&mut self) {
        let source = self.source.value.clone();
        if let Some(target) = self.target.selected_value() {
            let target = target.to_string();
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            self.version_rx = Some(rx);
            self.version.is_loading = true;
            self.version.loading_text = Some("Loading versions...".to_string());
            tokio::spawn(async move {
                let result =
                    tokio::task::spawn_blocking(move || cim::fetch_versions(&source, &target))
                        .await;
                match result {
                    Ok(Ok(versions)) => {
                        let _ = tx.send(Ok(versions)).await;
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(Err(e.to_string())).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string())).await;
                    }
                }
            });
        }
    }

    fn next_focus(&mut self) {
        self.focus = match self.focus {
            InitFocus::Target => InitFocus::Source,
            InitFocus::Source => InitFocus::Version,
            InitFocus::Version => InitFocus::Workspace,
            InitFocus::Workspace => InitFocus::Match,
            InitFocus::Match => InitFocus::Toggles,
            InitFocus::Toggles => InitFocus::Target,
        };
    }

    fn prev_focus(&mut self) {
        self.focus = match self.focus {
            InitFocus::Target => InitFocus::Toggles,
            InitFocus::Source => InitFocus::Target,
            InitFocus::Version => InitFocus::Source,
            InitFocus::Workspace => InitFocus::Version,
            InitFocus::Match => InitFocus::Workspace,
            InitFocus::Toggles => InitFocus::Match,
        };
    }
}

impl CommandPanel for InitPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Target dropdown
                Constraint::Length(3), // Source input
                Constraint::Length(3), // Version dropdown
                Constraint::Length(3), // Workspace input
                Constraint::Length(3), // Match input
                Constraint::Length(1), // no-mirror
                Constraint::Length(1), // force
                Constraint::Length(1), // verbose
                Constraint::Length(1), // install
                Constraint::Length(1), // full
                Constraint::Length(1), // no-sudo
                Constraint::Length(1), // symlink
                Constraint::Length(1), // yes
                Constraint::Length(2), // run button
                Constraint::Min(0),
            ])
            .split(area);

        self.target
            .draw(frame, rows[0], self.focus == InitFocus::Target);
        self.source
            .draw(frame, rows[1], self.focus == InitFocus::Source);
        self.version
            .draw(frame, rows[2], self.focus == InitFocus::Version);
        self.workspace
            .draw(frame, rows[3], self.focus == InitFocus::Workspace);
        self.match_pattern
            .draw(frame, rows[4], self.focus == InitFocus::Match);

        let in_toggles = self.focus == InitFocus::Toggles;
        Checkbox::new("[m] No mirror", self.no_mirror).draw(frame, rows[5], in_toggles);
        Checkbox::new("[f] Force", self.force).draw(frame, rows[6], in_toggles);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[7], in_toggles);
        Checkbox::new("[i] Install", self.install).draw(frame, rows[8], in_toggles);
        Checkbox::new("[u] Full", self.full).draw(frame, rows[9], in_toggles);
        Checkbox::new("[s] No sudo", self.no_sudo).draw(frame, rows[10], in_toggles);
        Checkbox::new("[l] Symlink", self.symlink).draw(frame, rows[11], in_toggles);
        Checkbox::new("[y] Yes", self.yes).draw(frame, rows[12], in_toggles);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Initialize").style(run_style),
            rows[13],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab => self.next_focus(),
            KeyCode::BackTab => self.prev_focus(),
            KeyCode::Enter => match self.focus {
                InitFocus::Target => {
                    if !self.targets_loaded {
                        self.load_targets();
                        self.targets_loaded = true;
                        return PanelAction::None;
                    }
                    if self.target.open {
                        self.target.toggle_open();
                        self.load_versions();
                    } else {
                        self.target.toggle_open();
                    }
                }
                InitFocus::Version => {
                    self.version.toggle_open();
                }
                _ => {
                    if let Some(target) = self.target.selected_value() {
                        let target = target.to_string();
                        let version = self.version.selected_value().map(|s| s.to_string());
                        let workspace = if self.workspace.value.is_empty() {
                            None
                        } else {
                            Some(self.workspace.value.clone())
                        };
                        let match_pattern = if self.match_pattern.value.is_empty() {
                            None
                        } else {
                            Some(self.match_pattern.value.clone())
                        };
                        let cv = self.cert_validation.selected_value().and_then(|v| {
                            if v == "default" {
                                None
                            } else {
                                Some(v.to_string())
                            }
                        });
                        return PanelAction::Run(CimCommand::Init {
                            target,
                            source: self.source.value.clone(),
                            version,
                            workspace,
                            no_mirror: self.no_mirror,
                            force: self.force,
                            match_pattern,
                            verbose: self.verbose,
                            install: self.install,
                            full: self.full,
                            no_sudo: self.no_sudo,
                            symlink: self.symlink,
                            yes: self.yes,
                            cert_validation: cv,
                        });
                    }
                }
            },
            _ => match self.focus {
                InitFocus::Target => {
                    if self.target.open {
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Down => self.target.select_next(),
                            KeyCode::Char('k') | KeyCode::Up => self.target.select_previous(),
                            _ => {}
                        }
                    } else if !self.targets_loaded {
                        self.load_targets();
                        self.targets_loaded = true;
                    }
                }
                InitFocus::Source => match key.code {
                    KeyCode::Char(c) => {
                        self.source.handle_key(KeyCode::Char(c));
                    }
                    KeyCode::Backspace => {
                        self.source.handle_key(KeyCode::Backspace);
                    }
                    _ => {}
                },
                InitFocus::Version => {
                    if self.version.open {
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Down => self.version.select_next(),
                            KeyCode::Char('k') | KeyCode::Up => self.version.select_previous(),
                            _ => {}
                        }
                    }
                }
                InitFocus::Workspace => match key.code {
                    KeyCode::Char(c) => {
                        self.workspace.handle_key(KeyCode::Char(c));
                    }
                    KeyCode::Backspace => {
                        self.workspace.handle_key(KeyCode::Backspace);
                    }
                    _ => {}
                },
                InitFocus::Match => match key.code {
                    KeyCode::Char(c) => {
                        self.match_pattern.handle_key(KeyCode::Char(c));
                    }
                    KeyCode::Backspace => {
                        self.match_pattern.handle_key(KeyCode::Backspace);
                    }
                    _ => {}
                },
                InitFocus::Toggles => match key.code {
                    KeyCode::Char('m') => self.no_mirror = !self.no_mirror,
                    KeyCode::Char('f') => self.force = !self.force,
                    KeyCode::Char('v') => self.verbose = !self.verbose,
                    KeyCode::Char('i') => self.install = !self.install,
                    KeyCode::Char('u') => self.full = !self.full,
                    KeyCode::Char('s') => self.no_sudo = !self.no_sudo,
                    KeyCode::Char('l') => self.symlink = !self.symlink,
                    KeyCode::Char('y') => self.yes = !self.yes,
                    _ => {}
                },
            },
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        match self.focus {
            InitFocus::Target | InitFocus::Version => {
                vec![
                    ("[Esc]", "back"),
                    ("[Tab]", "next"),
                    ("[Enter]", "open/select"),
                ]
            }
            InitFocus::Source | InitFocus::Workspace | InitFocus::Match => {
                vec![("[Esc]", "back"), ("[Tab]", "next"), ("[Enter]", "init")]
            }
            InitFocus::Toggles => {
                vec![
                    ("[Esc]", "back"),
                    ("[m]", "mirror"),
                    ("[f]", "force"),
                    ("[v]", "verbose"),
                    ("[Enter]", "init"),
                ]
            }
        }
    }

    fn check_async(&mut self) {
        if let Some(rx) = &mut self.target_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(targets) => {
                        self.target.items = targets;
                        self.target.is_loading = false;
                        self.target.loading_text = None;
                    }
                    Err(e) => {
                        self.target.is_loading = false;
                        self.target.loading_text = Some(format!("Error: {}", e));
                    }
                }
                self.target_rx = None;
            }
        }
        if let Some(rx) = &mut self.version_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(versions) => {
                        self.version.items = versions;
                        self.version.is_loading = false;
                        self.version.loading_text = None;
                    }
                    Err(e) => {
                        self.version.is_loading = false;
                        self.version.loading_text = Some(format!("Error: {}", e));
                    }
                }
                self.version_rx = None;
            }
        }
    }
}

// --- UpdatePanel ---

pub struct UpdatePanel {
    no_mirror: bool,
    verbose: bool,
    match_pattern: TextInput,
    cert_validation: DropdownSelect,
    in_text: bool,
}

impl UpdatePanel {
    pub fn new() -> Self {
        Self {
            no_mirror: false,
            verbose: false,
            match_pattern: TextInput::new("Match"),
            cert_validation: DropdownSelect::new(
                "Cert validation",
                vec!["default".to_string(), "os".to_string(), "none".to_string()],
            ),
            in_text: false,
        }
    }
}

impl CommandPanel for UpdatePanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        Checkbox::new("[m] No mirror", self.no_mirror).draw(frame, rows[0], !self.in_text);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[1], !self.in_text);
        self.match_pattern.draw(frame, rows[2], self.in_text);

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Update").style(run_style), rows[3]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.in_text = !self.in_text;
            }
            KeyCode::Enter => {
                let mp = if self.match_pattern.value.is_empty() {
                    None
                } else {
                    Some(self.match_pattern.value.clone())
                };
                let cv = self.cert_validation.selected_value().and_then(|v| {
                    if v == "default" {
                        None
                    } else {
                        Some(v.to_string())
                    }
                });
                return PanelAction::Run(CimCommand::Update {
                    no_mirror: self.no_mirror,
                    match_pattern: mp,
                    verbose: self.verbose,
                    cert_validation: cv,
                });
            }
            _ => {
                if self.in_text {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.match_pattern.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.match_pattern.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('m') => self.no_mirror = !self.no_mirror,
                        KeyCode::Char('v') => self.verbose = !self.verbose,
                        _ => {}
                    }
                }
            }
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[Tab]", "toggle input"),
            ("[m]", "no-mirror"),
            ("[v]", "verbose"),
            ("[Enter]", "update"),
        ]
    }
}

// --- AddPanel ---

enum AddFocus {
    Name,
    Url,
    Commit,
}

pub struct AddPanel {
    focus: AddFocus,
    name: TextInput,
    url: TextInput,
    commit: TextInput,
}

impl AddPanel {
    pub fn new() -> Self {
        Self {
            focus: AddFocus::Name,
            name: TextInput::new("Name"),
            url: TextInput::new("URL"),
            commit: TextInput::new("Commit"),
        }
    }
}

impl CommandPanel for AddPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        self.name
            .draw(frame, rows[0], matches!(self.focus, AddFocus::Name));
        self.url
            .draw(frame, rows[1], matches!(self.focus, AddFocus::Url));
        self.commit
            .draw(frame, rows[2], matches!(self.focus, AddFocus::Commit));

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Add").style(run_style), rows[3]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    AddFocus::Name => AddFocus::Url,
                    AddFocus::Url => AddFocus::Commit,
                    AddFocus::Commit => AddFocus::Name,
                };
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    AddFocus::Name => AddFocus::Commit,
                    AddFocus::Url => AddFocus::Name,
                    AddFocus::Commit => AddFocus::Url,
                };
            }
            KeyCode::Enter
                if !self.name.value.is_empty()
                    && !self.url.value.is_empty()
                    && !self.commit.value.is_empty() =>
            {
                return PanelAction::Run(CimCommand::Add {
                    name: self.name.value.clone(),
                    url: self.url.value.clone(),
                    commit: self.commit.value.clone(),
                });
            }
            KeyCode::Char(c) => match self.focus {
                AddFocus::Name => {
                    self.name.handle_key(KeyCode::Char(c));
                }
                AddFocus::Url => {
                    self.url.handle_key(KeyCode::Char(c));
                }
                AddFocus::Commit => {
                    self.commit.handle_key(KeyCode::Char(c));
                }
            },
            KeyCode::Backspace => match self.focus {
                AddFocus::Name => {
                    self.name.handle_key(KeyCode::Backspace);
                }
                AddFocus::Url => {
                    self.url.handle_key(KeyCode::Backspace);
                }
                AddFocus::Commit => {
                    self.commit.handle_key(KeyCode::Backspace);
                }
            },
            _ => {}
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[Tab]", "next field"),
            ("[Enter]", "add"),
        ]
    }
}
