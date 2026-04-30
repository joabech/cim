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

// --- DocsCreatePanel ---

enum DocsCreateFocus {
    Theme,
    Toggles,
}

pub struct DocsCreatePanel {
    focus: DocsCreateFocus,
    theme_input: TextInput,
    force: bool,
    symlink: bool,
    verbose: bool,
}

impl DocsCreatePanel {
    pub fn new() -> Self {
        Self {
            focus: DocsCreateFocus::Toggles,
            theme_input: TextInput::new("Theme").with_value("sphinx_rtd_theme"),
            force: false,
            symlink: false,
            verbose: false,
        }
    }
}

impl CommandPanel for DocsCreatePanel {
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

        self.theme_input
            .draw(frame, rows[0], matches!(self.focus, DocsCreateFocus::Theme));
        Checkbox::new("[f] Force", self.force).draw(frame, rows[1], false);
        Checkbox::new("[s] Symlink", self.symlink).draw(frame, rows[2], false);
        Checkbox::new("[v] Verbose", self.verbose).draw(frame, rows[3], false);

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Create").style(run_style), rows[4]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    DocsCreateFocus::Theme => DocsCreateFocus::Toggles,
                    DocsCreateFocus::Toggles => DocsCreateFocus::Theme,
                };
            }
            KeyCode::Enter => {
                return PanelAction::Run(CimCommand::DocsCreate {
                    force: self.force,
                    theme: self.theme_input.value.clone(),
                    symlink: self.symlink,
                    verbose: self.verbose,
                });
            }
            _ => {
                if matches!(self.focus, DocsCreateFocus::Theme) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.theme_input.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.theme_input.handle_key(KeyCode::Backspace);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('f') => self.force = !self.force,
                        KeyCode::Char('s') => self.symlink = !self.symlink,
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
            ("[f]", "force"),
            ("[s]", "symlink"),
            ("[Enter]", "create"),
        ]
    }
}

// --- DocsBuildPanel ---

pub struct DocsBuildPanel {
    format: String,
}

impl DocsBuildPanel {
    pub fn new() -> Self {
        Self {
            format: "html".to_string(),
        }
    }
}

impl CommandPanel for DocsBuildPanel {
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

        let html_label = if self.format == "html" {
            "  [h] Format: html  ◀"
        } else {
            "  [h] Format: html"
        };
        let pdf_label = if self.format == "pdf" {
            "  [p] Format: pdf   ◀"
        } else {
            "  [p] Format: pdf"
        };

        frame.render_widget(
            Paragraph::new(html_label).style(if self.format == "html" {
                theme.selected()
            } else {
                theme.text()
            }),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(pdf_label).style(if self.format == "pdf" {
                theme.selected()
            } else {
                theme.text()
            }),
            rows[1],
        );

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Build").style(run_style), rows[2]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Char('h') => self.format = "html".to_string(),
            KeyCode::Char('p') => self.format = "pdf".to_string(),
            KeyCode::Enter => {
                return PanelAction::Run(CimCommand::DocsBuild {
                    format: self.format.clone(),
                });
            }
            _ => {}
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[h]", "html"),
            ("[p]", "pdf"),
            ("[Enter]", "build"),
        ]
    }
}

// --- DocsServePanel ---

enum DocsServeFocus {
    Port,
    Host,
}

pub struct DocsServePanel {
    focus: DocsServeFocus,
    port: TextInput,
    host: TextInput,
}

impl DocsServePanel {
    pub fn new() -> Self {
        Self {
            focus: DocsServeFocus::Port,
            port: TextInput::new("Port").with_value("8000"),
            host: TextInput::new("Host").with_value("localhost"),
        }
    }
}

impl CommandPanel for DocsServePanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        self.port
            .draw(frame, rows[0], matches!(self.focus, DocsServeFocus::Port));
        self.host
            .draw(frame, rows[1], matches!(self.focus, DocsServeFocus::Host));

        let run_style = theme.primary_button(true);
        frame.render_widget(Paragraph::new("  [Enter] Serve").style(run_style), rows[2]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    DocsServeFocus::Port => DocsServeFocus::Host,
                    DocsServeFocus::Host => DocsServeFocus::Port,
                };
            }
            KeyCode::Enter => {
                let port: u16 = self.port.value.parse().unwrap_or(8000);
                return PanelAction::Run(CimCommand::DocsServe {
                    port,
                    host: self.host.value.clone(),
                });
            }
            KeyCode::Char(c) => match self.focus {
                DocsServeFocus::Port => {
                    self.port.handle_key(KeyCode::Char(c));
                }
                DocsServeFocus::Host => {
                    self.host.handle_key(KeyCode::Char(c));
                }
            },
            KeyCode::Backspace => match self.focus {
                DocsServeFocus::Port => {
                    self.port.handle_key(KeyCode::Backspace);
                }
                DocsServeFocus::Host => {
                    self.host.handle_key(KeyCode::Backspace);
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
            ("[Enter]", "serve"),
        ]
    }
}
