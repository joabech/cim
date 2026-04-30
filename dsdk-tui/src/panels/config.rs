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
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::cim::CimCommand;
use crate::components::TextInput;
use crate::panels::{CommandPanel, PanelAction};
use crate::theme::theme;

enum ConfigFocus {
    Actions,
    GetInput,
}

pub struct ConfigPanel {
    focus: ConfigFocus,
    get_input: TextInput,
    force: bool,
}

impl ConfigPanel {
    pub fn new() -> Self {
        Self {
            focus: ConfigFocus::Actions,
            get_input: TextInput::new("Get key"),
            force: false,
        }
    }

    fn make_cmd(&self, action: &str) -> CimCommand {
        CimCommand::Config {
            list: action == "list",
            get: if action == "get" {
                Some(self.get_input.value.clone())
            } else {
                None
            },
            show_path: action == "path",
            template: action == "template",
            create: action == "create",
            force: self.force,
            edit: action == "edit",
            validate: action == "validate",
        }
    }
}

impl CommandPanel for ConfigPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect) {
        let theme = theme();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);

        let actions = [
            ("l", "List"),
            ("p", "Path"),
            ("t", "Template"),
            ("v", "Validate"),
            ("c", "Create"),
            ("e", "Edit"),
        ];

        for (i, (key, label)) in actions.iter().enumerate() {
            let line = Line::from(vec![
                Span::styled(format!("  [{}] ", key), theme.key_badge()),
                Span::styled(label.to_string(), theme.text()),
            ]);
            frame.render_widget(Paragraph::new(line), rows[i]);
        }

        let force_indicator = if self.force { "[✓]" } else { "[ ]" };
        let force_line = Line::from(vec![
            Span::styled("  [f] ", theme.key_badge()),
            Span::styled(format!("Force {}", force_indicator), theme.text()),
        ]);
        frame.render_widget(Paragraph::new(force_line), rows[6]);

        let get_line = Line::from(vec![
            Span::styled("  [g] ", theme.key_badge()),
            Span::styled("Get (Enter key below, then press g)", theme.muted()),
        ]);
        frame.render_widget(Paragraph::new(get_line), rows[7]);

        self.get_input
            .draw(frame, rows[8], matches!(self.focus, ConfigFocus::GetInput));
    }

    fn handle_key(&mut self, key: KeyEvent) -> PanelAction {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    ConfigFocus::Actions => ConfigFocus::GetInput,
                    ConfigFocus::GetInput => ConfigFocus::Actions,
                };
            }
            _ => {
                if matches!(self.focus, ConfigFocus::GetInput) {
                    match key.code {
                        KeyCode::Char(c) => {
                            self.get_input.handle_key(KeyCode::Char(c));
                        }
                        KeyCode::Backspace => {
                            self.get_input.handle_key(KeyCode::Backspace);
                        }
                        KeyCode::Enter if !self.get_input.value.is_empty() => {
                            return PanelAction::Run(self.make_cmd("get"));
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('l') => return PanelAction::Run(self.make_cmd("list")),
                        KeyCode::Char('p') => return PanelAction::Run(self.make_cmd("path")),
                        KeyCode::Char('t') => return PanelAction::Run(self.make_cmd("template")),
                        KeyCode::Char('v') => return PanelAction::Run(self.make_cmd("validate")),
                        KeyCode::Char('c') => return PanelAction::Run(self.make_cmd("create")),
                        KeyCode::Char('e') => return PanelAction::Run(self.make_cmd("edit")),
                        KeyCode::Char('f') => self.force = !self.force,
                        KeyCode::Char('g') => {
                            if !self.get_input.value.is_empty() {
                                return PanelAction::Run(self.make_cmd("get"));
                            }
                            self.focus = ConfigFocus::GetInput;
                        }
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
            ("[l]", "list"),
            ("[p]", "path"),
            ("[t]", "template"),
            ("[v]", "validate"),
            ("[c]", "create"),
            ("[e]", "edit"),
        ]
    }
}
