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

enum ReleaseFocus {
    Tag,
    Include,
    Exclude,
    Toggles,
}

pub struct ReleasePanel {
    focus: ReleaseFocus,
    tag: TextInput,
    include: TextInput,
    exclude: TextInput,
    dry_run: bool,
    genconfig: bool,
}

impl ReleasePanel {
    pub fn new() -> Self {
        Self {
            focus: ReleaseFocus::Tag,
            tag: TextInput::new("Tag"),
            include: TextInput::new("Include"),
            exclude: TextInput::new("Exclude"),
            dry_run: false,
            genconfig: false,
        }
    }
}

impl CommandPanel for ReleasePanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

        self.tag
            .draw(frame, rows[0], matches!(self.focus, ReleaseFocus::Tag));
        self.include
            .draw(frame, rows[1], matches!(self.focus, ReleaseFocus::Include));
        self.exclude
            .draw(frame, rows[2], matches!(self.focus, ReleaseFocus::Exclude));
        Checkbox::new("[d] Dry run", self.dry_run).draw(
            frame,
            rows[3],
            matches!(self.focus, ReleaseFocus::Toggles),
        );
        Checkbox::new("[g] Genconfig", self.genconfig).draw(
            frame,
            rows[4],
            matches!(self.focus, ReleaseFocus::Toggles),
        );

        let run_style = theme.primary_button(true);
        frame.render_widget(
            Paragraph::new("  [Enter] Release").style(run_style),
            rows[5],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    ReleaseFocus::Tag => ReleaseFocus::Include,
                    ReleaseFocus::Include => ReleaseFocus::Exclude,
                    ReleaseFocus::Exclude => ReleaseFocus::Toggles,
                    ReleaseFocus::Toggles => ReleaseFocus::Tag,
                };
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    ReleaseFocus::Tag => ReleaseFocus::Toggles,
                    ReleaseFocus::Include => ReleaseFocus::Tag,
                    ReleaseFocus::Exclude => ReleaseFocus::Include,
                    ReleaseFocus::Toggles => ReleaseFocus::Exclude,
                };
            }
            KeyCode::Enter => {
                let tag = if self.tag.value.is_empty() {
                    None
                } else {
                    Some(self.tag.value.clone())
                };
                let include: Vec<String> = self
                    .include
                    .value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let exclude: Vec<String> = self
                    .exclude
                    .value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                return PanelAction::Run(CimCommand::Release {
                    tag,
                    genconfig: self.genconfig,
                    include,
                    exclude,
                    dry_run: self.dry_run,
                });
            }
            _ => match self.focus {
                ReleaseFocus::Tag => match key.code {
                    KeyCode::Char(c) => {
                        self.tag.handle_key(KeyCode::Char(c));
                    }
                    KeyCode::Backspace => {
                        self.tag.handle_key(KeyCode::Backspace);
                    }
                    _ => {}
                },
                ReleaseFocus::Include => match key.code {
                    KeyCode::Char(c) => {
                        self.include.handle_key(KeyCode::Char(c));
                    }
                    KeyCode::Backspace => {
                        self.include.handle_key(KeyCode::Backspace);
                    }
                    _ => {}
                },
                ReleaseFocus::Exclude => match key.code {
                    KeyCode::Char(c) => {
                        self.exclude.handle_key(KeyCode::Char(c));
                    }
                    KeyCode::Backspace => {
                        self.exclude.handle_key(KeyCode::Backspace);
                    }
                    _ => {}
                },
                ReleaseFocus::Toggles => match key.code {
                    KeyCode::Char('d') => self.dry_run = !self.dry_run,
                    KeyCode::Char('g') => self.genconfig = !self.genconfig,
                    _ => {}
                },
            },
        }
        PanelAction::None
    }

    fn status_keys(&self) -> Vec<(&str, &str)> {
        vec![
            ("[Esc]", "back"),
            ("[Tab]", "next field"),
            ("[d]", "dry-run"),
            ("[g]", "genconfig"),
            ("[Enter]", "release"),
        ]
    }
}
