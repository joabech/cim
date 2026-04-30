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
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use ratatui::style::Style;

use crate::theme::{symbols, theme};

pub struct Checkbox {
    pub checked: bool,
    pub label: String,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            checked,
            label: label.into(),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, is_focused: bool) {
        let theme = theme();
        let checkbox = if self.checked {
            format!("[{}]", symbols::CHECKBOX_CHECKED)
        } else {
            format!("[{}]", symbols::CHECKBOX_UNCHECKED)
        };

        let checkbox_style = theme.checkbox(self.checked, is_focused);
        let label_style = if is_focused {
            theme.text().add_modifier(Modifier::BOLD)
        } else if self.checked {
            theme.text()
        } else {
            theme.muted()
        };

        let spans = vec![
            Span::styled(checkbox, checkbox_style),
            Span::styled(" ", Style::default()),
            Span::styled(self.label.clone(), label_style),
        ];

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, area);
    }
}
