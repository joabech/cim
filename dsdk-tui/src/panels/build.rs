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

// --- ForeachPanel ---

enum ForeachFocus {
    Command,
    Match,
}

pub struct ForeachPanel {
    focus: ForeachFocus,
    command: TextInput,
    match_pattern: TextInput,
}

impl ForeachPanel {
    pub fn new() -> Self {
        Self {
            focus: ForeachFocus::Command,
            command: TextInput::new("Command"),
            match_pattern: TextInput::new("Match"),
        }
    }

    fn is_text_focused(&self) -> bool {
        true
    }
}

impl CommandPanel for ForeachPanel {
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

        self.command
            .draw(frame, rows[0], matches!(self.focus, ForeachFocus::Command));
        self.match_pattern
            .draw(frame, rows[1], matches!(self.focus, ForeachFocus::Match));

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Execute").style(run_style),
            rows[2],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    ForeachFocus::Command => ForeachFocus::Match,
                    ForeachFocus::Match => ForeachFocus::Command,
                };
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    ForeachFocus::Command => ForeachFocus::Match,
                    ForeachFocus::Match => ForeachFocus::Command,
                };
            }
            KeyCode::Enter if !self.command.value.is_empty() => {
                let mp = if self.match_pattern.value.is_empty() {
                    None
                } else {
                    Some(self.match_pattern.value.clone())
                };
                return PanelAction::Run(CimCommand::Foreach {
                    command: self.command.value.clone(),
                    match_pattern: mp,
                });
            }
            KeyCode::Char(c) if self.is_text_focused() => match self.focus {
                ForeachFocus::Command => {
                    self.command.handle_key(KeyCode::Char(c));
                }
                ForeachFocus::Match => {
                    self.match_pattern.handle_key(KeyCode::Char(c));
                }
            },
            KeyCode::Backspace if self.is_text_focused() => match self.focus {
                ForeachFocus::Command => {
                    self.command.handle_key(KeyCode::Backspace);
                }
                ForeachFocus::Match => {
                    self.match_pattern.handle_key(KeyCode::Backspace);
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
            ("[Enter]", "execute"),
        ]
    }
}

// --- MakefilePanel ---

pub struct MakefilePanel {
    no_dividers: bool,
}

impl MakefilePanel {
    pub fn new() -> Self {
        Self { no_dividers: false }
    }
}

impl CommandPanel for MakefilePanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        Checkbox::new("No dividers", self.no_dividers).draw(frame, rows[0], true);

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Generate").style(run_style),
            rows[1],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Char('d') | KeyCode::Char(' ') => {
                self.no_dividers = !self.no_dividers;
            }
            KeyCode::Enter => {
                return PanelAction::Run(CimCommand::Makefile {
                    no_dividers: self.no_dividers,
                });
            }
            _ => {}
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[d]", "no-dividers"),
            ("[Enter]", "generate"),
        ]
    }
}
