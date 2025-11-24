use crate::parsing::janusql_parser::WindowType;
use crate::parsing::janusql_parser::R2SOperator;

pub struct HistoricalWindow {
    pub window_name: String,
    pub stream_name: String,
    pub width: Option<u64>,
    pub slide: Option<u64>,
    pub offset: Option<u64>,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub window_type: WindowType,
}
