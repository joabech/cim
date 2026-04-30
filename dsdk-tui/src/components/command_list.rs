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

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::theme::{symbols, theme, Theme};

pub struct CommandEntry {
    pub label: String,
    pub is_header: bool,
    pub panel_index: Option<usize>,
    pub requires_workspace: bool,
}

pub struct CommandList {
    pub entries: Vec<CommandEntry>,
    pub selected: usize,
}

impl CommandList {
    pub fn new() -> Self {
        let mut entries = Vec::new();
        let mut panel_idx = 0usize;

        // (category, [(command_name, requires_workspace)])
        let categories: &[(&str, &[(&str, bool)])] = &[
            (
                "WORKSPACE",
                &[("init", false), ("update", true), ("add", true)],
            ),
            ("BUILD", &[("foreach", true), ("makefile", true)]),
            (
                "INSTALL",
                &[
                    ("os-deps", true),
                    ("pip", true),
                    ("toolchains", true),
                    ("tools", true),
                ],
            ),
            (
                "DOCS",
                &[("create", true), ("build", true), ("serve", true)],
            ),
            ("RELEASE", &[("release", true)]),
            ("CONFIG", &[("config", false)]),
            (
                "UTILS",
                &[
                    ("hash-copy-files", false),
                    ("hash-toolchains", false),
                    ("sync-copy-files", false),
                    ("update", false),
                ],
            ),
        ];

        for (cat, commands) in categories {
            entries.push(CommandEntry {
                label: cat.to_string(),
                is_header: true,
                panel_index: None,
                requires_workspace: false,
            });
            for (cmd, req_ws) in *commands {
                entries.push(CommandEntry {
                    label: cmd.to_string(),
                    is_header: false,
                    panel_index: Some(panel_idx),
                    requires_workspace: *req_ws,
                });
                panel_idx += 1;
            }
        }

        let selected = entries.iter().position(|e| !e.is_header).unwrap_or(0);

        Self { entries, selected }
    }

    pub fn selected_panel_index(&self) -> Option<usize> {
        self.entries.get(self.selected).and_then(|e| e.panel_index)
    }

    pub fn is_selected_disabled(&self, in_workspace: bool) -> bool {
        self.entries
            .get(self.selected)
            .is_some_and(|e| e.requires_workspace && !in_workspace)
    }

    pub fn select_next(&mut self) {
        let mut next = self.selected + 1;
        while next < self.entries.len() {
            if !self.entries[next].is_header {
                self.selected = next;
                return;
            }
            next += 1;
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected == 0 {
            return;
        }
        let mut prev = self.selected - 1;
        loop {
            if !self.entries[prev].is_header {
                self.selected = prev;
                return;
            }
            if prev == 0 {
                return;
            }
            prev -= 1;
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, is_focused: bool, in_workspace: bool) {
        let theme = theme();

        let border_style = if is_focused {
            theme.focus_border()
        } else {
            theme.unfocus_border()
        };

        let block = Block::default()
            .title(" Commands ")
            .title_style(theme.title())
            .borders(Borders::ALL)
            .border_type(Theme::BORDER_TYPE)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines: Vec<Line> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                if entry.is_header {
                    Line::from(Span::styled(format!(" {}", entry.label), theme.muted()))
                } else {
                    let is_selected = i == self.selected;
                    let disabled = entry.requires_workspace && !in_workspace;
                    let indicator = if is_selected { symbols::SELECTION } else { " " };
                    let style = if disabled {
                        theme.disabled()
                    } else if is_selected {
                        theme.selected()
                    } else {
                        theme.text()
                    };
                    Line::from(Span::styled(
                        format!(" {} {}", indicator, entry.label),
                        style,
                    ))
                }
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}
