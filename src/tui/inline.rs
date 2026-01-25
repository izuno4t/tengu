#[derive(Debug, Clone)]
pub struct InlineRenderState {
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub footer_row: Option<u16>,
    pub input_rows: u16,
    pub status_rows: u16,
    pub min_log_rows: u16,
    pub dirty: bool,
}

impl Default for InlineRenderState {
    fn default() -> Self {
        Self {
            cursor_row: 0,
            cursor_col: 0,
            footer_row: None,
            input_rows: 0,
            status_rows: 0,
            min_log_rows: 3,
            dirty: true,
        }
    }
}
