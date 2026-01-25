use std::io::{self, Stdout, Write};

use crossterm::terminal::size;
use unicode_width::UnicodeWidthStr;

use crate::tui::ansi;
use crate::tui::state::AppState;

pub fn draw(stdout: &mut Stdout, state: &mut AppState) -> io::Result<()> {
    let (term_width, _term_height) = size()?;
    let width = term_width as usize;

    let spacer_height = 1u16;
    let divider_height = 1u16;
    let input_rows = state.input_row_count();
    let input_height = input_rows.saturating_add(1);
    let status_height = 1u16;
    let app_status_height = 1u16;
    let help_height = if state.suggestions.is_empty() {
        0
    } else {
        state.suggestions.lines().count() as u16
    };
    let log_height = (state.log_lines.len() as u16).max(3);

    let total_height = log_height
        .saturating_add(spacer_height)
        .saturating_add(status_height)
        .saturating_add(divider_height)
        .saturating_add(input_height)
        .saturating_add(help_height)
        .saturating_add(app_status_height);

    let mut lines: Vec<String> = Vec::with_capacity(total_height as usize);
    let log_block = state.visible_log(log_height);
    if !log_block.is_empty() {
        for line in log_block.lines() {
            lines.push(fit_width(line, width));
        }
    }
    while lines.len() < log_height as usize {
        lines.push(String::new());
    }

    lines.push(String::new());

    let spinner_frames = ["👺   ", " 👺  ", "  👺 ", "   👺"];
    let spinner = if state.status_state == "running" {
        spinner_frames[(state.tick / 6) as usize % spinner_frames.len()]
    } else {
        ""
    };
    let status_line = format!("• status: {} {}", state.status_detail, spinner);
    lines.push(colorize_line(&status_line, width, ansi::set_fg(crossterm::style::Color::Yellow)));

    lines.push(colorize_line(
        &"─".repeat(width),
        width,
        ansi::set_fg(crossterm::style::Color::Grey),
    ));

    let input_lines: Vec<&str> = state.input.split('\n').collect();
    for (idx, line) in input_lines.iter().enumerate() {
        let prefix = if idx == 0 { "> " } else { "  " };
        let input_line = format!("{}{}", prefix, line);
        lines.push(fit_width(&input_line, width));
    }
    while lines.len() < (log_height + spacer_height + status_height + divider_height + input_rows) as usize {
        lines.push(String::new());
    }
    lines.push(colorize_line(
        &"─".repeat(width),
        width,
        ansi::set_fg(crossterm::style::Color::Grey),
    ));

    if help_height > 0 {
        for line in state.suggestions.lines() {
            lines.push(fit_width(line, width));
        }
    }

    let app_left = format!("model: {} • build {}", state.status_model, state.status_build);
    let app_right = if state.suggestions.is_empty() {
        "Ctrl+C to quit • ? for shortcuts"
    } else {
        ""
    };
    let app_status = align_right(&app_left, app_right, width);
    lines.push(colorize_line(
        &app_status,
        width,
        ansi::set_fg(crossterm::style::Color::Grey),
    ));

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

    let input_row = origin
        .saturating_add(log_height)
        .saturating_add(spacer_height)
        .saturating_add(status_height)
        .saturating_add(divider_height)
        .saturating_add(input_rows.saturating_sub(1));
    let last_line = input_lines.last().copied().unwrap_or("");
    let cursor_offset = UnicodeWidthStr::width(last_line) as u16;
    let cursor_col = 2u16.saturating_add(cursor_offset);
    let cursor_row_ansi = input_row.saturating_add(1);
    let cursor_col_ansi = cursor_col.saturating_add(1);

    state.inline.cursor_row = input_row;
    state.inline.cursor_col = cursor_col;
    state.inline.footer_row = Some(origin.saturating_add(total_height.saturating_sub(1)));
    state.inline.input_rows = input_rows;
    state.inline.status_rows = status_height;
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
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    let mut current = 0usize;
    let mut out = String::new();
    for ch in text.chars() {
        let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
        if current + ch_width > width {
            break;
        }
        out.push(ch);
        current += ch_width;
    }
    out
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
    let left_w = UnicodeWidthStr::width(left);
    let right_w = UnicodeWidthStr::width(right);
    if left_w + 1 + right_w > width {
        return fit_width(left, width);
    }
    let pad = width.saturating_sub(left_w + right_w);
    format!("{}{}{}", left, " ".repeat(pad), right)
}
