use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "maid_state", rename_all = "snake_case")]
pub enum MaidState {
    Alive,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Maid {
    pub id: Uuid,
    pub identity: String,
    pub started_at: DateTime<Utc>,
    pub checked_in_at: DateTime<Utc>,
    pub state: MaidState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "task_type", rename_all = "snake_case")]
pub enum TaskType {
    SendEmail,
    ResizeImage,
    Cleanup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "task_state", rename_all = "snake_case")]
pub enum TaskState {
    Ready,
    Active,
    Failed,
    Panicked,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub id: Uuid,
    pub r#type: TaskType,
    pub state: TaskState,
    pub payload: sqlx::types::JsonValue,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub taken_by: Option<String>,
}