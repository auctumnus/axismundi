use std::{collections::HashMap, fmt::Display, sync::LazyLock};

use serde::{Deserialize, Serialize};

use crate::{config::CONFIG, err::AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub changes: String,
    pub input_words: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_words: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_polling: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStep {
    pub rule: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFailure {
    pub message: String,
    pub rule: String,
    pub original_word: String,
    pub current_word: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub rule_names: Vec<String>,
    pub output_words: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intermediate_words: Option<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traces: Option<HashMap<String, Vec<TraceStep>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<RuleFailure>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Error {
    ParseError {
        message: String,
        line_number: u32,
        column_number: u32,
    },
    InvalidExpression {
        message: String,
        rule: String,
        expression: String,
        expression_number: u32,
    },
    AnalysisError {
        message: String,
    },
    RuntimeError {
        message: String,
    },
    Timeout {
        message: String,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ParseError {
                message,
                line_number,
                column_number,
            } => {
                write!(
                    f,
                    "Parse error at line {}, column {}: {}",
                    line_number, column_number, message
                )
            }
            Error::InvalidExpression {
                message,
                rule,
                expression,
                expression_number,
            } => {
                write!(
                    f,
                    "Invalid expression in rule '{}', expression {}: {}. Expression was: '{}'",
                    rule, expression_number, message, expression
                )
            }
            Error::AnalysisError { message } => {
                write!(f, "Analysis error: {}", message)
            }
            Error::RuntimeError { message } => {
                write!(f, "Runtime error: {}", message)
            }
            Error::Timeout { message } => {
                write!(f, "Timeout error: {}", message)
            }
        }
    }
}

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

pub async fn send_scv1(request: &Request) -> AppResult<Result<Response, Error>> {
    let response = CLIENT
        .post(format!("{}/scv1", CONFIG.lexurgy.url))
        .timeout(std::time::Duration::from_secs(60))
        .header("Authorization", CONFIG.lexurgy.api_key.clone())
        .json(request)
        .send()
        .await;

    println!("Received response from Lexurgy: {:?}", response);

    match response {
        Ok(response) => {
            if response.status() == 400 {
                let error = response.json::<Error>().await?;
                Ok(Err(error))
            } else {
                let response = response.json::<Response>().await?;
                Ok(Ok(response))
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Ok(Err(Error::Timeout {
                    message: "The request timed out".to_string(),
                }))
            } else {
                Ok(Err(Error::RuntimeError {
                    message: format!("An error occurred while sending the request: {}", e),
                }))
            }
        }
    }
}

pub async fn run_sound_changes(
    changes: String,
    input_words: Vec<String>,
    start_at: Option<String>,
    stop_before: Option<String>,
    trace_words: Option<Vec<String>>,
) -> AppResult<Result<Response, Error>> {
    let request = Request {
        changes,
        input_words,
        trace_words,
        start_at,
        stop_before,
        allow_polling: None,
    };

    send_scv1(&request).await
}
