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
use crate::components::{Checkbox, TextInput};
use crate::panels::{CommandPanel, PanelAction};
use crate::theme::theme;

// --- OsDepsPanel ---

pub struct OsDepsPanel {
    yes: bool,
    no_sudo: bool,
}

impl OsDepsPanel {
    pub fn new() -> Self {
        Self {
            yes: false,
            no_sudo: false,
        }
    }
}

impl CommandPanel for OsDepsPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        Checkbox::new("[y] Yes", self.yes).draw(frame, rows[0], false);
        Checkbox::new("[s] No sudo", self.no_sudo).draw(frame, rows[1], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Install").style(run_style),
            rows[2],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Char('y') => self.yes = !self.yes,
            KeyCode::Char('s') => self.no_sudo = !self.no_sudo,
            KeyCode::Enter => {
                return PanelAction::Run(CimCommand::InstallOsDeps {
                    yes: self.yes,
                    no_sudo: self.no_sudo,
                });
            }
            _ => {}
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[y]", "yes"),
            ("[s]", "no-sudo"),
            ("[Enter]", "install"),
        ]
    }
}

// --- PipPanel ---

enum PipFocus {
    Profile,
    Toggles,
}

pub struct PipPanel {
    focus: PipFocus,
    profile: TextInput,
    force: bool,
    symlink: bool,
    list_profiles: bool,
}

impl PipPanel {
    pub fn new() -> Self {
        Self {
            focus: PipFocus::Toggles,
            profile: TextInput::new("Profile"),
            force: false,
            symlink: false,
            list_profiles: false,
        }
    }
}

impl CommandPanel for PipPanel {
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

        self.profile
            .draw(frame, rows[0], matches!(self.focus, PipFocus::Profile));
        Checkbox::new("[f] Force", self.force).draw(frame, rows[1], false);
        Checkbox::new("[s] Symlink", self.symlink).draw(frame, rows[2], false);
        Checkbox::new("[l] List profiles", self.list_profiles).draw(frame, rows[3], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Install").style(run_style),
            rows[4],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    PipFocus::Profile => PipFocus::Toggles,
                    PipFocus::Toggles => PipFocus::Profile,
                };
            }
            KeyCode::Enter => {
                let profile = if self.profile.value.is_empty() {
                    None
                } else {
                    Some(self.profile.value.clone())
                };
                return PanelAction::Run(CimCommand::InstallPip {
                    force: self.force,
                    symlink: self.symlink,
                    profile,
                    list_profiles: self.list_profiles,
                });
            }
            _ => {
                if matches!(self.focus, PipFocus::Profile) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.profile.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.profile.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('f') => self.force = !self.force,
                        KeyCode::Char('s') => self.symlink = !self.symlink,
                        KeyCode::Char('l') => self.list_profiles = !self.list_profiles,
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
            ("[f]", "force"),
            ("[s]", "symlink"),
            ("[Enter]", "install"),
        ]
    }
}

// --- ToolchainsPanel ---

pub struct ToolchainsPanel {
    force: bool,
    symlink: bool,
    verbose: bool,
}

impl ToolchainsPanel {
    pub fn new() -> Self {
        Self {
            force: false,
            symlink: false,
            verbose: false,
        }
    }
}

impl CommandPanel for ToolchainsPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        Checkbox::new("[f] Force", self.force).draw(frame, rows[0], false);
        Checkbox::new("[s] Symlink", self.symlink).draw(frame, rows[1], false);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[2], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Install").style(run_style),
            rows[3],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Char('f') => self.force = !self.force,
            KeyCode::Char('s') => self.symlink = !self.symlink,
            KeyCode::Char('v') => self.verbose = !self.verbose,
            KeyCode::Enter => {
                return PanelAction::Run(CimCommand::InstallToolchains {
                    force: self.force,
                    symlink: self.symlink,
                    verbose: self.verbose,
                });
            }
            _ => {}
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[f]", "force"),
            ("[s]", "symlink"),
            ("[v]", "verbose"),
            ("[Enter]", "install"),
        ]
    }
}

// --- ToolsPanel ---

enum ToolsFocus {
    Name,
    Toggles,
}

pub struct ToolsPanel {
    focus: ToolsFocus,
    name: TextInput,
    list: bool,
    all: bool,
    force: bool,
}

impl ToolsPanel {
    pub fn new() -> Self {
        Self {
            focus: ToolsFocus::Toggles,
            name: TextInput::new("Tool name"),
            list: false,
            all: false,
            force: false,
        }
    }
}

impl CommandPanel for ToolsPanel {
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

        self.name
            .draw(frame, rows[0], matches!(self.focus, ToolsFocus::Name));
        Checkbox::new("[l] List", self.list).draw(frame, rows[1], false);
        Checkbox::new("[a] All", self.all).draw(frame, rows[2], false);
        Checkbox::new("[f] Force", self.force).draw(frame, rows[3], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Install").style(run_style),
            rows[4],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    ToolsFocus::Name => ToolsFocus::Toggles,
                    ToolsFocus::Toggles => ToolsFocus::Name,
                };
            }
            KeyCode::Enter => {
                let name = if self.name.value.is_empty() {
                    None
                } else {
                    Some(self.name.value.clone())
                };
                return PanelAction::Run(CimCommand::InstallTools {
                    name,
                    list: self.list,
                    all: self.all,
                    force: self.force,
                });
            }
            _ => {
                if matches!(self.focus, ToolsFocus::Name) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.name.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.name.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('l') => self.list = !self.list,
                        KeyCode::Char('a') => self.all = !self.all,
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
            ("[Tab]", "toggle input"),
            ("[l]", "list"),
            ("[a]", "all"),
            ("[Enter]", "install"),
        ]
    }
}
