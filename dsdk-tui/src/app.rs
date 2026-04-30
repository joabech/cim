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

use anyhow::Result;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, ScrollbarState, Wrap};
use std::path::Path;
use tokio::sync::mpsc;

use crate::cim::{self, CimCommand};
use crate::components::command_list::CommandList;
use crate::events::handle_event;
use crate::panels::build::{ForeachPanel, MakefilePanel};
use crate::panels::config::ConfigPanel;
use crate::panels::docs::{DocsBuildPanel, DocsCreatePanel, DocsServePanel};
use crate::panels::install::{OsDepsPanel, PipPanel, ToolchainsPanel, ToolsPanel};
use crate::panels::release::ReleasePanel;
use crate::panels::utils::{
    HashCopyFilesPanel, HashToolchainsPanel, SyncCopyFilesPanel, UtilsUpdatePanel,
};
use crate::panels::workspace::{AddPanel, InitPanel, UpdatePanel};
use crate::panels::{CommandPanel, PanelAction};
use crate::ui::draw;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    CommandList,
    Options,
    Output,
}

pub struct App {
    pub focus: Focus,
    pub in_workspace: bool,
    pub command_list: CommandList,
    pub panels: Vec<Box<dyn CommandPanel>>,
    pub output_text: String,
    pub output_scroll: u16,
    pub output_pane_height: u16,
    pub output_pane_width: u16,
    pub output_scrollbar_state: ScrollbarState,
    pub output_maximized: bool,
    pub status_message: Option<String>,
    output_rx: Option<mpsc::Receiver<String>>,
}

fn find_workspace() -> bool {
    let mut dir = std::env::current_dir().ok();
    while let Some(d) = dir {
        if d.join(".workspace").is_file() {
            return true;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    false
}

impl App {
    pub fn new() -> Self {
        let in_workspace = find_workspace();

        let panels: Vec<Box<dyn CommandPanel>> = vec![
            // WORKSPACE (indices 0-2)
            Box::new(InitPanel::new()),
            Box::new(UpdatePanel::new()),
            Box::new(AddPanel::new()),
            // BUILD (indices 3-4)
            Box::new(ForeachPanel::new()),
            Box::new(MakefilePanel::new()),
            // INSTALL (indices 5-8)
            Box::new(OsDepsPanel::new()),
            Box::new(PipPanel::new()),
            Box::new(ToolchainsPanel::new()),
            Box::new(ToolsPanel::new()),
            // DOCS (indices 9-11)
            Box::new(DocsCreatePanel::new()),
            Box::new(DocsBuildPanel::new()),
            Box::new(DocsServePanel::new()),
            // RELEASE (index 12)
            Box::new(ReleasePanel::new()),
            // CONFIG (index 13)
            Box::new(ConfigPanel::new()),
            // UTILS (indices 14-17)
            Box::new(HashCopyFilesPanel::new()),
            Box::new(HashToolchainsPanel::new()),
            Box::new(SyncCopyFilesPanel::new()),
            Box::new(UtilsUpdatePanel),
        ];

        Self {
            focus: Focus::CommandList,
            in_workspace,
            command_list: CommandList::new(),
            panels,
            output_text: String::new(),
            output_scroll: 0,
            output_pane_height: 10,
            output_pane_width: 76,
            output_scrollbar_state: ScrollbarState::default(),
            output_maximized: false,
            status_message: None,
            output_rx: None,
        }
    }

    pub async fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> Result<()> {
        loop {
            terminal.draw(|f| draw(f, self))?;

            if handle_event(self)? {
                return Ok(());
            }

            if let Some(idx) = self.command_list.selected_panel_index() {
                self.panels[idx].check_async();
            }

            self.check_output();
        }
    }

    pub fn active_panel(&self) -> Option<&dyn CommandPanel> {
        self.command_list
            .selected_panel_index()
            .map(|idx| self.panels[idx].as_ref())
    }

    pub fn handle_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Back => {
                self.focus = Focus::CommandList;
            }
            PanelAction::Run(cmd) => {
                self.execute_command(cmd);
            }
        }
    }

    pub fn execute_command(&mut self, cmd: CimCommand) {
        let label = cmd.label().to_string();

        match cim::spawn_command(&cmd) {
            Ok(mut child) => {
                self.status_message = Some(format!("Running 'cim {}'...", label));

                self.output_text.clear();
                self.output_text
                    .push_str(&format!("=== cim {} ===\n\n", label));

                let (tx, rx) = mpsc::channel::<String>(100);
                self.output_rx = Some(rx);

                if let Some(stdout) = child.stdout.take() {
                    let tx_stdout = tx.clone();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncBufReadExt, BufReader};
                        let reader = BufReader::new(stdout);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx_stdout.send(line).await;
                        }
                    });
                }

                if let Some(stderr) = child.stderr.take() {
                    let tx_stderr = tx.clone();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncBufReadExt, BufReader};
                        let reader = BufReader::new(stderr);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx_stderr.send(format!("ERROR: {}", line)).await;
                        }
                    });
                }

                tokio::spawn(async move {
                    match child.wait().await {
                        Ok(status) => {
                            let msg = if status.success() {
                                format!("\n=== {} completed successfully ===", label)
                            } else {
                                format!(
                                    "\n=== {} failed with exit code: {:?} ===",
                                    label,
                                    status.code()
                                )
                            };
                            let _ = tx.send(msg).await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(format!("\n=== Failed to wait for process: {} ===", e))
                                .await;
                        }
                    }
                });
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to start: {}", e));
            }
        }
    }

    fn check_output(&mut self) {
        if let Some(rx) = &mut self.output_rx {
            let mut new_content = false;
            while let Ok(line) = rx.try_recv() {
                self.output_text.push_str(&line);
                self.output_text.push('\n');
                new_content = true;
            }
            if new_content {
                let total_rendered = self.rendered_row_count();
                let visible = self.output_pane_height.saturating_sub(2);
                self.output_scroll = total_rendered.saturating_sub(visible);
                self.sync_scrollbar();
            }
        }
    }

    pub fn scroll_output_up(&mut self, lines: u16) {
        self.output_scroll = self.output_scroll.saturating_sub(lines);
        self.sync_scrollbar();
    }

    pub fn scroll_output_down(&mut self, lines: u16) {
        let total_rendered = self.rendered_row_count();
        let visible = self.output_pane_height.saturating_sub(2);
        let max_scroll = total_rendered.saturating_sub(visible);
        self.output_scroll = (self.output_scroll + lines).min(max_scroll);
        self.sync_scrollbar();
    }

    pub fn scroll_output_top(&mut self) {
        self.output_scroll = 0;
        self.sync_scrollbar();
    }

    pub fn scroll_output_bottom(&mut self) {
        let total_rendered = self.rendered_row_count();
        let visible = self.output_pane_height.saturating_sub(2);
        self.output_scroll = total_rendered.saturating_sub(visible);
        self.sync_scrollbar();
    }

    fn rendered_row_count(&self) -> u16 {
        let text = Text::from(self.output_text.as_str());
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .line_count(self.output_pane_width) as u16
    }

    pub fn sync_scrollbar(&mut self) {
        let total = self.rendered_row_count() as usize;
        self.output_scrollbar_state =
            ScrollbarState::new(total).position(self.output_scroll as usize);
    }
}
