use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SnapshotsQuery {
    pub codes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TicksQuery {
    pub code: String,
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KBarsQuery {
    pub code: String,
    pub start: String,
    pub end: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PositionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}
