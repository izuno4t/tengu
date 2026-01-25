use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;

use crossterm::style::Color;

#[derive(Debug, Deserialize)]
struct ThemeConfig {
    user: String,
    assistant: String,
    system: String,
    status: String,
    queue: String,
    heading: String,
    inline_code: String,
    divider: String,
    footer: String,
}

#[derive(Debug, Deserialize, Default)]
struct ThemeConfigOpt {
    user: Option<String>,
    assistant: Option<String>,
    system: Option<String>,
    status: Option<String>,
    queue: Option<String>,
    heading: Option<String>,
    inline_code: Option<String>,
    divider: Option<String>,
    footer: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub user: Color,
    pub assistant: Color,
    pub system: Color,
    pub status: Color,
    pub queue: Color,
    pub heading: Color,
    pub inline_code: Color,
    pub divider: Color,
    pub footer: Color,
}

pub static THEME: Lazy<Theme> = Lazy::new(|| {
    let raw = include_str!("theme.toml");
    let mut config: ThemeConfig = toml::from_str(raw).unwrap_or_else(|_| default_theme());
    if let Some(override_config) = load_home_override() {
        apply_override(&mut config, override_config);
    }
    let map = color_map();
    Theme {
        user: resolve_color(&config.user, &map),
        assistant: resolve_color(&config.assistant, &map),
        system: resolve_color(&config.system, &map),
        status: resolve_color(&config.status, &map),
        queue: resolve_color(&config.queue, &map),
        heading: resolve_color(&config.heading, &map),
        inline_code: resolve_color(&config.inline_code, &map),
        divider: resolve_color(&config.divider, &map),
        footer: resolve_color(&config.footer, &map),
    }
});

fn default_theme() -> ThemeConfig {
    ThemeConfig {
        user: "green".to_string(),
        assistant: "white".to_string(),
        system: "white".to_string(),
        status: "yellow".to_string(),
        queue: "dark_grey".to_string(),
        heading: "cyan".to_string(),
        inline_code: "cyan".to_string(),
        divider: "grey".to_string(),
        footer: "grey".to_string(),
    }
}

fn load_home_override() -> Option<ThemeConfigOpt> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::PathBuf::from(home).join(".tengu").join("theme.toml");
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn apply_override(config: &mut ThemeConfig, override_config: ThemeConfigOpt) {
    if let Some(value) = override_config.user {
        config.user = value;
    }
    if let Some(value) = override_config.assistant {
        config.assistant = value;
    }
    if let Some(value) = override_config.system {
        config.system = value;
    }
    if let Some(value) = override_config.status {
        config.status = value;
    }
    if let Some(value) = override_config.queue {
        config.queue = value;
    }
    if let Some(value) = override_config.heading {
        config.heading = value;
    }
    if let Some(value) = override_config.inline_code {
        config.inline_code = value;
    }
    if let Some(value) = override_config.divider {
        config.divider = value;
    }
    if let Some(value) = override_config.footer {
        config.footer = value;
    }
}

fn color_map() -> HashMap<&'static str, Color> {
    HashMap::from([
        ("black", Color::Black),
        ("dark_grey", Color::DarkGrey),
        ("gray", Color::Grey),
        ("grey", Color::Grey),
        ("red", Color::Red),
        ("dark_red", Color::DarkRed),
        ("green", Color::Green),
        ("dark_green", Color::DarkGreen),
        ("yellow", Color::Yellow),
        ("dark_yellow", Color::DarkYellow),
        ("blue", Color::Blue),
        ("dark_blue", Color::DarkBlue),
        ("magenta", Color::Magenta),
        ("dark_magenta", Color::DarkMagenta),
        ("cyan", Color::Cyan),
        ("dark_cyan", Color::DarkCyan),
        ("white", Color::White),
    ])
}

fn resolve_color(name: &str, map: &HashMap<&'static str, Color>) -> Color {
    let key = name.trim().to_ascii_lowercase();
    map.get(key.as_str()).copied().unwrap_or(Color::White)
}
