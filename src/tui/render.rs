use std::io::{self, Stdout, Write};

use crossterm::terminal::size;
use once_cell::sync::Lazy;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::tui::ansi;
use crate::tui::state::{AppState, LogRole};
use crate::tui::THEME;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static SYNTAX_THEME: Lazy<Theme> = Lazy::new(|| {
    let theme_set = ThemeSet::load_defaults();
    theme_set
        .themes
        .get("InspiredGitHub")
        .cloned()
        .or_else(|| theme_set.themes.values().next().cloned())
        .unwrap_or_default()
});

pub fn draw(stdout: &mut Stdout, state: &mut AppState) -> io::Result<()> {
    let (term_width, _term_height) = size()?;
    let width = term_width as usize;

    let spacer_height = 1u16;
    let divider_height = 1u16;
    let input_rows = state.input_row_count();
    let input_height = input_rows.saturating_add(1);
    let app_status_height = 1u16;
    let help_height = if state.suggestions.is_empty() {
        0
    } else {
        state.suggestions.lines().count() as u16
    };
    let mut log_height = (state.log_lines.len() as u16).max(3);
    if log_height >= 5 {
        state.inline.min_log_rows = 5;
    }
    log_height = log_height.max(state.inline.min_log_rows);

    let total_height = log_height
        .saturating_add(spacer_height)
        .saturating_add(divider_height)
        .saturating_add(input_height)
        .saturating_add(help_height)
        .saturating_add(app_status_height);

    let mut lines: Vec<String> = Vec::with_capacity(total_height as usize);
    lines.extend(build_log_lines(state, log_height, width));
    lines.extend(build_status_lines(state, width));

    lines.push(colorize_line(
        &"─".repeat(width),
        width,
        ansi::set_fg(THEME.divider),
    ));

    let input_lines: Vec<&str> = state.input.split('\n').collect();
    for (idx, line) in input_lines.iter().enumerate() {
        let prefix = if idx == 0 { "> " } else { "  " };
        let input_line = format!("{}{}", prefix, line);
        lines.push(fit_width(&input_line, width));
    }
    while lines.len()
        < (log_height + spacer_height + divider_height + input_rows) as usize
    {
        lines.push(String::new());
    }

    lines.push(colorize_line(
        &"─".repeat(width),
        width,
        ansi::set_fg(THEME.divider),
    ));

    if help_height > 0 {
        for line in state.suggestions.lines() {
            lines.push(fit_width(line, width));
        }
    }

    let origin = state.origin_y;
    for (idx, line) in lines.iter().enumerate() {
        let row = origin.saturating_add(idx as u16);
        let row_ansi = row.saturating_add(1);
        write!(
            stdout,
            "{}{}{}",
            ansi::move_to(row_ansi, 1),
            ansi::clear_line(),
            line
        )?;
    }

    let app_left = format!("model: {} • build {}", state.status_model, state.status_build);
    let app_right = if state.suggestions.is_empty() {
        "Ctrl+C to quit • ? for shortcuts"
    } else {
        ""
    };
    let app_status = align_right(&app_left, app_right, width);
    let footer_row = origin.saturating_add(total_height.saturating_sub(1));
    write!(
        stdout,
        "{}{}{}",
        ansi::move_to(footer_row.saturating_add(1), 1),
        ansi::clear_line(),
        colorize_line(&app_status, width, ansi::set_fg(THEME.footer))
    )?;

    let input_row = origin
        .saturating_add(log_height)
        .saturating_add(spacer_height)
        .saturating_add(divider_height)
        .saturating_add(input_rows.saturating_sub(1));
    let last_line = input_lines.last().copied().unwrap_or("");
    let cursor_offset = UnicodeWidthStr::width(last_line) as u16;
    let cursor_col = 2u16.saturating_add(cursor_offset);
    let cursor_row_ansi = input_row.saturating_add(1);
    let cursor_col_ansi = cursor_col.saturating_add(1);

    state.inline.cursor_row = input_row;
    state.inline.cursor_col = cursor_col;
    state.inline.footer_row = Some(footer_row);
    state.inline.input_rows = input_rows;
    state.inline.status_rows = 0;
    state.inline.dirty = false;

    write!(
        stdout,
        "{}{}",
        ansi::move_to(cursor_row_ansi, cursor_col_ansi),
        ansi::show_cursor()
    )?;
    stdout.flush()?;

    Ok(())
}

fn fit_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let lines = wrap_ansi_line(text, width);
    lines.into_iter().next().unwrap_or_default()
}

fn colorize_line(text: &str, width: usize, prefix: String) -> String {
    let mut out = String::new();
    out.push_str(&prefix);
    out.push_str(&fit_width(text, width));
    out.push_str(&ansi::reset());
    out
}

fn align_right(left: &str, right: &str, width: usize) -> String {
    if right.is_empty() {
        return fit_width(left, width);
    }
    let right_w = visible_width(right);
    if right_w >= width {
        return fit_width(right, width);
    }
    let left_max = width.saturating_sub(right_w + 1);
    let left_trim = fit_width(left, left_max);
    let left_w = visible_width(&left_trim);
    let pad = width.saturating_sub(left_w + right_w);
    format!("{}{}{}", left_trim, " ".repeat(pad), right)
}

fn build_log_lines(state: &AppState, height: u16, width: usize) -> Vec<String> {
    if height == 0 {
        return Vec::new();
    }
    let queue_lines = build_queue_lines(state, width);
    let min_log = state.inline.min_log_rows.max(3).min(height);
    let queue_take = if height > min_log {
        let available = height.saturating_sub(min_log);
        available.min(queue_lines.len() as u16)
    } else {
        0
    };
    let log_height = height.saturating_sub(queue_take);
    let mut lines = Vec::with_capacity(height as usize);

    if log_height > 0 {
        let visible = state.visible_log_lines(log_height);
        if !visible.is_empty() {
            let mut rendered = render_log_lines(&visible, width);
            if rendered.len() > log_height as usize {
                rendered = rendered[rendered.len() - log_height as usize..].to_vec();
            }
            lines.extend(rendered);
        }
        while lines.len() < log_height as usize {
            lines.push(String::new());
        }
    }

    lines.extend(queue_lines.into_iter().take(queue_take as usize));
    lines
}

fn build_queue_lines(state: &AppState, width: usize) -> Vec<String> {
    if state.queue.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let header = format!("queued: {}", state.queue.len());
    lines.push(colorize_line(&header, width, ansi::set_fg(THEME.queue)));
    for item in state.queue.iter() {
        let entry = format!("  {}", item.text);
        lines.push(colorize_line(&entry, width, ansi::set_fg(THEME.queue)));
    }
    lines
}

fn build_status_lines(state: &AppState, width: usize) -> Vec<String> {
    let spinner_frames = ["👺   ", " 👺  ", "  👺 ", "   👺"];
    let spinner = if state.status_state == "running" {
        spinner_frames[(state.tick / 6) as usize % spinner_frames.len()]
    } else {
        "    "
    };
    let (bullet, bullet_color) = if state.status_state == "running" {
        let frames = ["●", "◐", "◓", "◑", "◒"];
        let bullet = frames[(state.tick / 4) as usize % frames.len()];
        let color = if (state.tick / 4) % 2 == 0 {
            THEME.status_pulse
        } else {
            THEME.status
        };
        (bullet, color)
    } else {
        ("•", THEME.status)
    };
    let status_line = format!(
        "{}{}{} status: {} {}",
        ansi::set_fg(bullet_color),
        bullet,
        ansi::reset(),
        state.status_detail,
        spinner
    );
    vec![colorize_line(&status_line, width, ansi::set_fg(THEME.status))]
}

fn render_log_lines(lines: &[crate::tui::state::LogLine], width: usize) -> Vec<String> {
    let mut output = Vec::new();
    let mut buffer = String::new();

    for line in lines {
        match line.role {
            LogRole::User => {
                if !buffer.is_empty() {
                    output.extend(render_markdown_lines(&buffer, width));
                    buffer.clear();
                }
                let styled = colorize_line(&line.text, width, ansi::set_fg(THEME.user));
                output.extend(wrap_ansi_line(&styled, width));
            }
            LogRole::Assistant | LogRole::System => {
                if line.text.is_empty() {
                    if !buffer.is_empty() {
                        output.extend(render_markdown_lines(&buffer, width));
                        buffer.clear();
                    }
                    output.push(String::new());
                    continue;
                }
                if !buffer.is_empty() {
                    buffer.push('\n');
                }
                buffer.push_str(&line.text);
            }
        }
    }

    if !buffer.is_empty() {
        let collapsed = collapse_thought_blocks(&buffer);
        output.extend(render_markdown_lines(&collapse_error_blocks(&collapsed), width));
    }

    output
}

fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if let Some('[') = chars.peek().copied() {
                chars.next();
                while let Some(next) = chars.next() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        width += UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    width
}

fn collapse_thought_blocks(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_thought = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("思考:") || trimmed.eq_ignore_ascii_case("thoughts:") {
            if !in_thought {
                out.push("思考:（折りたたみ）".to_string());
                in_thought = true;
            }
            continue;
        }
        if in_thought {
            if trimmed.is_empty() {
                in_thought = false;
            }
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

fn render_markdown_lines(markdown: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut in_heading = false;
    let mut heading_level = 0u32;
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_lines: Vec<String> = Vec::new();
    let mut list_prefix_pending = false;
    let mut _in_paragraph = false;
    let mut blockquote_depth = 0u16;

    let normalized = normalize_markdown(markdown);
    let parser = Parser::new_ext(&normalized, Options::all());
    for event in parser {
        match event {
            Event::Start(Tag::Paragraph) => {
                _in_paragraph = true;
                if !current.trim().is_empty() {
                    lines.extend(wrap_ansi_line(&current, width));
                    current.clear();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                _in_paragraph = false;
                if !current.trim().is_empty() {
                    lines.extend(wrap_ansi_line(&current, width));
                    current.clear();
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                heading_level = level as u32;
                current.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                let prefix = "#".repeat(heading_level as usize);
                let line = format!("{} {}", prefix, current.trim());
                let styled = colorize_line(&line, width, ansi::set_fg(THEME.heading));
                lines.extend(wrap_ansi_line(&styled, width));
                current.clear();
                in_heading = false;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_lines.clear();
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                    CodeBlockKind::Indented => None,
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                let lang = code_lang.as_deref().unwrap_or("");
                let syntax = if lang.is_empty() {
                    SYNTAX_SET.find_syntax_plain_text()
                } else {
                    SYNTAX_SET
                        .find_syntax_by_token(lang)
                        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
                };
                let mut highlighter = HighlightLines::new(syntax, &SYNTAX_THEME);
                for line in code_lines.drain(..) {
                    match highlighter.highlight_line(&line, &SYNTAX_SET) {
                        Ok(ranges) => {
                            let escaped = as_24_bit_terminal_escaped(&ranges, false);
                            lines.extend(wrap_ansi_line(&escaped, width));
                        }
                        Err(_) => {
                            lines.extend(wrap_ansi_line(&line, width));
                        }
                    }
                }
                in_code_block = false;
                code_lang = None;
            }
            Event::Start(Tag::BlockQuote) => {
                blockquote_depth = blockquote_depth.saturating_add(1);
            }
            Event::End(TagEnd::BlockQuote) => {
                blockquote_depth = blockquote_depth.saturating_sub(1);
                if !current.trim().is_empty() {
                    lines.extend(wrap_ansi_line(&current, width));
                    current.clear();
                }
            }
            Event::Start(Tag::Item) => {
                list_prefix_pending = true;
            }
            Event::End(TagEnd::Item) => {
                list_prefix_pending = false;
                if !current.trim().is_empty() {
                    lines.extend(style_task_line(&current, width));
                    current.clear();
                }
            }
            Event::Text(text) => {
                if in_code_block {
                    code_lines.extend(text.lines().map(|line| line.to_string()));
                    continue;
                }
                if in_heading {
                    current.push_str(&text);
                    continue;
                }
                if list_prefix_pending {
                    if blockquote_depth > 0 {
                        current.push_str("> ");
                    }
                    current.push_str("- ");
                    list_prefix_pending = false;
                }
                if blockquote_depth > 0 && current.is_empty() {
                    current.push_str("> ");
                }
                current.push_str(&text);
            }
            Event::Code(text) => {
                if list_prefix_pending {
                    if blockquote_depth > 0 {
                        current.push_str("> ");
                    }
                    current.push_str("- ");
                    list_prefix_pending = false;
                }
                if blockquote_depth > 0 && current.is_empty() {
                    current.push_str("> ");
                }
                let styled = format!("{}{}{}", ansi::set_fg(THEME.inline_code), text, ansi::reset());
                current.push_str(&styled);
            }
            Event::SoftBreak => {
                if in_code_block {
                    code_lines.push(String::new());
                } else {
                    lines.extend(wrap_ansi_line(&current, width));
                    current.clear();
                }
            }
            Event::HardBreak => {
                lines.extend(wrap_ansi_line(&current, width));
                current.clear();
            }
            _ => {}
        }
    }

    if !current.trim().is_empty() {
        lines.extend(style_task_line(&current, width));
    }

    lines
}

fn collapse_error_blocks(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_detail = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("error:") || trimmed.starts_with("error ") || trimmed.starts_with("ERROR") {
            let styled = format!(
                "{}{}{}",
                ansi::set_fg(THEME.error),
                trimmed,
                ansi::reset()
            );
            out.push(styled);
            in_detail = true;
            continue;
        }
        if in_detail {
            if trimmed.is_empty() {
                in_detail = false;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if in_detail {
        out.push(format!(
            "{}{}{}",
            ansi::set_fg(THEME.error_detail),
            "details:（折りたたみ）",
            ansi::reset()
        ));
    }
    out.join("\n")
}

fn normalize_markdown(input: &str) -> String {
    let mut output = input.replace("。- ", "。\n- ");
    output = output.replace("。 - ", "。\n- ");
    output = output.replace(".- ", ".\n- ");
    output = output.replace(". - ", ".\n- ");
    output = output.replace(":- ", ":\n- ");
    output = output.replace(": - ", ":\n- ");
    output
}

fn wrap_ansi_line(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    let mut chars = text.chars().peekable();
    let mut last_sgr: Option<String> = None;
    let mut pending_space: Option<(String, usize)> = None;

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            let mut esc = String::new();
            esc.push(ch);
            if let Some('[') = chars.peek().copied() {
                esc.push('[');
                chars.next();
                while let Some(next) = chars.next() {
                    esc.push(next);
                    if ('@'..='~').contains(&next) {
                        if next == 'm' {
                            last_sgr = Some(esc.clone());
                        }
                        break;
                    }
                }
            }
            current.push_str(&esc);
            continue;
        }

        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > width {
            if let Some((space_segment, _space_width)) = pending_space.take() {
                current.push_str(&space_segment);
            }
            if !current.is_empty() {
                if last_sgr.is_some() {
                    current.push_str(&ansi::reset());
                }
                lines.push(current);
            } else {
                lines.push(String::new());
            }
            current = String::new();
            current_width = 0;
            if let Some(sgr) = &last_sgr {
                current.push_str(sgr);
            }
        }

        if ch == ' ' {
            pending_space = Some((ch.to_string(), ch_width));
            continue;
        }

        if let Some((space_segment, space_width)) = pending_space.take() {
            current.push_str(&space_segment);
            current_width += space_width;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if let Some((space_segment, space_width)) = pending_space.take() {
        if current_width + space_width <= width {
            current.push_str(&space_segment);
        }
    }

    if !current.is_empty() {
        if last_sgr.is_some() {
            current.push_str(&ansi::reset());
        }
        lines.push(current);
    } else if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn style_task_line(line: &str, width: usize) -> Vec<String> {
    let trimmed = line.trim_start();
    let prefix_len = line.len() - trimmed.len();
    let (prefix_space, _rest) = line.split_at(prefix_len);
    let patterns = [("- [ ] ", false), ("- [x] ", true), ("- [X] ", true)];
    for (pattern, checked) in patterns {
        if trimmed.starts_with(pattern) {
            let content = trimmed.strip_prefix(pattern).unwrap_or("");
            let box_color = if checked {
                THEME.todo_checked
            } else {
                THEME.todo_unchecked
            };
            let checkbox = if checked { "[x]" } else { "[ ]" };
            let styled = format!(
                "{}- {}{} {}",
                prefix_space,
                ansi::set_fg(box_color),
                checkbox,
                ansi::reset()
            );
            let full = format!("{styled}{content}");
            return wrap_ansi_line(&full, width);
        }
    }
    wrap_ansi_line(line, width)
}
