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

use crate::cim::CimCommand;
use crate::components::Checkbox;
use crate::components::TextInput;
use crate::panels::{CommandPanel, PanelAction};
use crate::theme::theme;

// --- HashCopyFilesPanel ---

enum HashCopyFilesFocus {
    File,
    Toggles,
}

pub struct HashCopyFilesPanel {
    focus: HashCopyFilesFocus,
    file: TextInput,
    dry_run: bool,
    verbose: bool,
    add_missing: bool,
}

impl HashCopyFilesPanel {
    pub fn new() -> Self {
        Self {
            focus: HashCopyFilesFocus::Toggles,
            file: TextInput::new("File"),
            dry_run: false,
            verbose: false,
            add_missing: false,
        }
    }
}

impl CommandPanel for HashCopyFilesPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        self.file.draw(
            frame,
            rows[0],
            matches!(self.focus, HashCopyFilesFocus::File),
        );
        Checkbox::new("[d] Dry run", self.dry_run).draw(frame, rows[1], false);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[2], false);
        Checkbox::new("[a] Add missing", self.add_missing).draw(frame, rows[3], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Run").style(run_style), rows[4]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    HashCopyFilesFocus::File => HashCopyFilesFocus::Toggles,
                    HashCopyFilesFocus::Toggles => HashCopyFilesFocus::File,
                };
            }
            KeyCode::Enter => {
                let file = if self.file.value.is_empty() {
                    None
                } else {
                    Some(self.file.value.clone())
                };
                return PanelAction::Run(CimCommand::UtilsHashCopyFiles {
                    file,
                    dry_run: self.dry_run,
                    verbose: self.verbose,
                    add_missing: self.add_missing,
                });
            }
            _ => {
                if matches!(self.focus, HashCopyFilesFocus::File) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.file.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.file.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('d') => self.dry_run = !self.dry_run,
                        KeyCode::Char('v') => self.verbose = !self.verbose,
                        KeyCode::Char('a') => self.add_missing = !self.add_missing,
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
            ("[Tab]", "file input"),
            ("[d]", "dry-run"),
            ("[v]", "verbose"),
            ("[Enter]", "run"),
        ]
    }
}

// --- HashToolchainsPanel ---

enum HashToolchainsFocus {
    File,
    Toggles,
}

pub struct HashToolchainsPanel {
    focus: HashToolchainsFocus,
    file: TextInput,
    dry_run: bool,
    verbose: bool,
    add_missing: bool,
}

impl HashToolchainsPanel {
    pub fn new() -> Self {
        Self {
            focus: HashToolchainsFocus::Toggles,
            file: TextInput::new("Toolchain"),
            dry_run: false,
            verbose: false,
            add_missing: false,
        }
    }
}

impl CommandPanel for HashToolchainsPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        self.file.draw(
            frame,
            rows[0],
            matches!(self.focus, HashToolchainsFocus::File),
        );
        Checkbox::new("[d] Dry run", self.dry_run).draw(frame, rows[1], false);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[2], false);
        Checkbox::new("[a] Add missing", self.add_missing).draw(frame, rows[3], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Run").style(run_style), rows[4]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    HashToolchainsFocus::File => HashToolchainsFocus::Toggles,
                    HashToolchainsFocus::Toggles => HashToolchainsFocus::File,
                };
            }
            KeyCode::Enter => {
                let file = if self.file.value.is_empty() {
                    None
                } else {
                    Some(self.file.value.clone())
                };
                return PanelAction::Run(CimCommand::UtilsHashToolchains {
                    file,
                    dry_run: self.dry_run,
                    verbose: self.verbose,
                    add_missing: self.add_missing,
                });
            }
            _ => {
                if matches!(self.focus, HashToolchainsFocus::File) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.file.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.file.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('d') => self.dry_run = !self.dry_run,
                        KeyCode::Char('v') => self.verbose = !self.verbose,
                        KeyCode::Char('a') => self.add_missing = !self.add_missing,
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
            ("[Tab]", "file input"),
            ("[d]", "dry-run"),
            ("[v]", "verbose"),
            ("[Enter]", "run"),
        ]
    }
}

// --- SyncCopyFilesPanel ---

enum SyncCopyFilesFocus {
    File,
    Toggles,
}

pub struct SyncCopyFilesPanel {
    focus: SyncCopyFilesFocus,
    file: TextInput,
    dry_run: bool,
    verbose: bool,
    force: bool,
}

impl SyncCopyFilesPanel {
    pub fn new() -> Self {
        Self {
            focus: SyncCopyFilesFocus::Toggles,
            file: TextInput::new("File"),
            dry_run: false,
            verbose: false,
            force: false,
        }
    }
}

impl CommandPanel for SyncCopyFilesPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        self.file.draw(
            frame,
            rows[0],
            matches!(self.focus, SyncCopyFilesFocus::File),
        );
        Checkbox::new("[d] Dry run", self.dry_run).draw(frame, rows[1], false);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[2], false);
        Checkbox::new("[f] Force", self.force).draw(frame, rows[3], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Run").style(run_style), rows[4]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    SyncCopyFilesFocus::File => SyncCopyFilesFocus::Toggles,
                    SyncCopyFilesFocus::Toggles => SyncCopyFilesFocus::File,
                };
            }
            KeyCode::Enter => {
                let file = if self.file.value.is_empty() {
                    None
                } else {
                    Some(self.file.value.clone())
                };
                return PanelAction::Run(CimCommand::UtilsSyncCopyFiles {
                    file,
                    dry_run: self.dry_run,
                    verbose: self.verbose,
                    force: self.force,
                });
            }
            _ => {
                if matches!(self.focus, SyncCopyFilesFocus::File) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.file.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.file.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('d') => self.dry_run = !self.dry_run,
                        KeyCode::Char('v') => self.verbose = !self.verbose,
                        KeyCode::Char('f') => self.force = !self.force,
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
            ("[Tab]", "file input"),
            ("[d]", "dry-run"),
            ("[v]", "verbose"),
            ("[Enter]", "run"),
        ]
    }
}

// --- UtilsUpdatePanel ---

pub struct UtilsUpdatePanel;

impl CommandPanel for UtilsUpdatePanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0)])
            .split(area);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Update cim").style(run_style),
            rows[0],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        if key.code == KeyCode::Enter {
            return PanelAction::Run(CimCommand::UtilsUpdate);
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![("[Esc]", "back"), ("[Enter]", "update")]
    }
}
