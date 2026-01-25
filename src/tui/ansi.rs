use crossterm::style::Color;

pub const ESC: &str = "\x1b";

pub fn clear_line() -> String {
    format!("{ESC}[K")
}

pub fn carriage_return() -> String {
    "\r".to_string()
}

pub fn move_to(row: u16, col: u16) -> String {
    format!("{ESC}[{};{}H", row, col)
}

pub fn hide_cursor() -> String {
    format!("{ESC}[?25l")
}

pub fn show_cursor() -> String {
    format!("{ESC}[?25h")
}

pub fn set_fg(color: Color) -> String {
    let code = match color {
        Color::Black => 30,
        Color::DarkGrey => 90,
        Color::Red => 31,
        Color::DarkRed => 91,
        Color::Green => 32,
        Color::DarkGreen => 92,
        Color::Yellow => 33,
        Color::DarkYellow => 93,
        Color::Blue => 34,
        Color::DarkBlue => 94,
        Color::Magenta => 35,
        Color::DarkMagenta => 95,
        Color::Cyan => 36,
        Color::DarkCyan => 96,
        Color::White => 37,
        Color::Grey => 90,
        Color::Rgb { r, g, b } => return format!("{ESC}[38;2;{r};{g};{b}m"),
        Color::AnsiValue(value) => return format!("{ESC}[38;5;{value}m"),
        _ => 39,
    };
    format!("{ESC}[{code}m")
}

pub fn reset() -> String {
    format!("{ESC}[0m")
}
