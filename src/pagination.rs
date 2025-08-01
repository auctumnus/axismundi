#[derive(Serialize, Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PaginatedRequest {
    pub limit: usize,
    pub cursor: Option<String>,
}