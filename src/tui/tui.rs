// TUI module
// Terminal User Interface

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io::{self, Stdout};
use tokio::runtime::Runtime;

use crate::mcp::{list_tools_http, list_tools_stdio, McpStore};

pub struct App {
    pub should_quit: bool,
    pub message: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            message: "Press q to quit, m to load MCP tools".to_string(),
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

        let result = self.run_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| {
                let area = frame.size();
                let block = Block::default()
                    .title("Tengu TUI")
                    .borders(Borders::ALL);
                let text = Paragraph::new(self.message.as_str());
                frame.render_widget(block, area);
                frame.render_widget(text, area);
            })?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') {
                        self.should_quit = true;
                    }
                    if key.code == KeyCode::Char('m') {
                        self.message = load_mcp_tools_view();
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn load_mcp_tools_view() -> String {
    let path = McpStore::default_path();
    let config = match McpStore::load(&path) {
        Ok(config) => config,
        Err(err) => {
            return format!("failed to load mcp config: {}", err);
        }
    };
    if config.mcp_servers.is_empty() {
        return "no mcp servers".to_string();
    }

    let mut lines = Vec::new();
    for (name, server) in config.mcp_servers.iter() {
        lines.push(format!("[{}]", name));
        if server.url.is_some() {
            let result = Runtime::new()
                .map_err(anyhow::Error::from)
                .and_then(|rt| rt.block_on(list_tools_http(server)));
            match result {
                Ok(tools) => {
                    for tool in tools {
                        lines.push(format!("@{}/{}", name, tool.name));
                    }
                }
                Err(err) => {
                    lines.push(format!("error: {}", err));
                }
            }
            continue;
        }
        match list_tools_stdio(server) {
            Ok(tools) => {
                for tool in tools {
                    lines.push(format!("@{}/{}", name, tool.name));
                }
            }
            Err(err) => {
                lines.push(format!("error: {}", err));
            }
        }
    }

    lines.join("\n")
}
