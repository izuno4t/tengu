use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::tui::state::AppState;

pub fn draw(frame: &mut Frame, state: &mut AppState) {
    let mut area = frame.size();
    if state.origin_y < area.height {
        area.y = state.origin_y;
        area.height = area.height.saturating_sub(state.origin_y);
    }
    frame.render_widget(Clear, area);

    let spacer_height = 1u16;
    let divider_height = 1u16;
    let input_height = 2u16;
    let status_height = 1u16;
    let app_status_height = 1u16;
    let min_fixed = spacer_height
        .saturating_add(divider_height)
        .saturating_add(input_height)
        .saturating_add(status_height)
        .saturating_add(app_status_height);
    let available_help = area.height.saturating_sub(min_fixed);
    let help_height = if state.suggestions.is_empty() {
        0
    } else {
        let lines = state.suggestions.lines().count() as u16;
        lines.min(available_help)
    };
    let available_log = area
        .height
        .saturating_sub(spacer_height)
        .saturating_sub(divider_height)
        .saturating_sub(input_height)
        .saturating_sub(status_height)
        .saturating_sub(help_height)
        .saturating_sub(app_status_height);
    let log_lines = state.log_lines.len() as u16;
    let desired_log = log_lines.max(3);
    let log_height = available_log.min(desired_log);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(log_height),
                Constraint::Length(spacer_height),
                Constraint::Length(status_height),
                Constraint::Length(divider_height),
                Constraint::Length(input_height),
                Constraint::Length(help_height),
                Constraint::Length(app_status_height),
            ]
            .as_ref(),
        )
        .split(area);

    if layout[0].height > 0 && layout[0].width > 0 {
        let output_text =
            Paragraph::new(state.visible_log(layout[0].height)).wrap(Wrap { trim: false });
        frame.render_widget(output_text, layout[0]);
    }

    let spinner_frames = ["👺   ", " 👺  ", "  👺 ", "   👺"];
    let spinner = if state.status_state == "running" {
        spinner_frames[(state.tick / 6) as usize % spinner_frames.len()]
    } else {
        ""
    };
    let status_line = format!("• status: {} {}", state.status_detail, spinner);
    let status_text = status_line;
    if layout[2].height > 0 && layout[2].width > 0 {
        let status = Paragraph::new(status_text)
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: false });
        frame.render_widget(status, layout[2]);
    }

    if layout[3].height > 0 && layout[3].width > 0 {
        let divider_top = Paragraph::new("─".repeat(layout[3].width as usize))
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(divider_top, layout[3]);
    }

    let input_block = Block::default().borders(Borders::BOTTOM);

    frame.render_widget(&input_block, layout[4]);
    let input_area = input_block.inner(layout[4]);
    if input_area.height > 0 && input_area.width > 0 {
        let input_line = Rect {
            x: input_area.x,
            y: input_area.y,
            width: input_area.width,
            height: 1,
        };
        let input_text = Paragraph::new(format!("> {}", state.input)).wrap(Wrap { trim: false });
        frame.render_widget(input_text, input_line);
    }

    if help_height > 0 && layout[5].height > 0 && layout[5].width > 0 {
        let help_text = Paragraph::new(state.suggestions.as_str()).wrap(Wrap { trim: false });
        frame.render_widget(help_text, layout[5]);
    }

    if layout[6].height > 0 && layout[6].width > 0 {
        let app_status_text =
            format!("model: {} • build {}", state.status_model, state.status_build);
        let app_status = Paragraph::new(app_status_text)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: false });
        frame.render_widget(app_status, layout[6]);

        let help_text = if state.suggestions.is_empty() {
            "Ctrl+C to quit • ? for shortcuts"
        } else {
            ""
        };
        let help = Paragraph::new(help_text)
            .alignment(Alignment::Right)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: false });
        frame.render_widget(help, layout[6]);
    }

    if input_area.height > 0 && input_area.width > 0 {
        let cursor_offset = UnicodeWidthStr::width(state.input.as_str()) as u16;
        let cursor_x = input_area
            .x
            .saturating_add(2)
            .saturating_add(cursor_offset)
            .min(input_area.x.saturating_add(input_area.width.saturating_sub(1)));
        let cursor_y = input_area.y.min(area.y.saturating_add(area.height.saturating_sub(1)));
        frame.set_cursor(cursor_x, cursor_y);
    }
}
