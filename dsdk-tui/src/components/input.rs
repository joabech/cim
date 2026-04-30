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

use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::theme::{theme, Theme};

pub struct TextInput {
    pub value: String,
    pub label: String,
}

impl TextInput {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            value: String::new(),
            label: label.into(),
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char(c) => {
                self.value.push(c);
                true
            }
            KeyCode::Backspace => {
                self.value.pop();
                true
            }
            _ => false,
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, is_focused: bool) {
        let theme = theme();
        let style = theme.input(is_focused);

        let title_style = if is_focused {
            theme.title()
        } else {
            theme.muted()
        };

        let block = Block::default()
            .title(self.label.as_str())
            .title_style(title_style)
            .borders(Borders::ALL)
            .border_type(Theme::BORDER_TYPE)
            .border_style(style);

        let text = if is_focused {
            format!("{}▌", self.value)
        } else {
            self.value.clone()
        };

        let paragraph = Paragraph::new(text).block(block).style(style);
        frame.render_widget(paragraph, area);
    }
}
