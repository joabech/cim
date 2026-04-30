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
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::theme::{symbols, theme, Theme};

pub struct DropdownSelect {
    pub items: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub label: String,
    pub is_loading: bool,
    pub loading_text: Option<String>,
}

impl DropdownSelect {
    pub fn new(label: impl Into<String>, items: Vec<String>) -> Self {
        Self {
            items,
            selected: 0,
            open: false,
            label: label.into(),
            is_loading: false,
            loading_text: None,
        }
    }

    pub fn toggle_open(&mut self) {
        if !self.items.is_empty() {
            self.open = !self.open;
        }
    }

    pub fn select_next(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_value(&self) -> Option<&str> {
        self.items.get(self.selected).map(|s| s.as_str())
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, is_focused: bool) -> Option<Rect> {
        let theme = theme();
        let style = theme.dropdown(is_focused);

        let display_text = if self.is_loading {
            self.loading_text
                .clone()
                .unwrap_or_else(|| "Loading...".to_string())
        } else if self.items.is_empty() {
            "No items available".to_string()
        } else {
            self.items[self.selected].clone()
        };

        let display_text = if is_focused {
            format!("{} {}", display_text, symbols::DROPDOWN_CLOSED)
        } else {
            display_text
        };

        let block = Block::default()
            .title(self.label.as_str())
            .title_style(if is_focused {
                theme.title()
            } else {
                theme.muted()
            })
            .borders(Borders::ALL)
            .border_type(Theme::BORDER_TYPE)
            .border_style(style);

        let paragraph = Paragraph::new(display_text).block(block).style(style);
        frame.render_widget(paragraph, area);

        if self.open && is_focused && !self.items.is_empty() {
            let dropdown_height = self.items.len().min(10) as u16 + 2;
            Some(Rect::new(
                area.x,
                area.y + area.height,
                area.width,
                dropdown_height,
            ))
        } else {
            None
        }
    }
}
