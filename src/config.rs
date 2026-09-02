use std::sync::LazyLock;

use serde::Deserialize;

fn default_region() -> String {
    "us-east-1".to_string()
}

#[derive(Clone, Deserialize, Debug)]
pub struct S3Config {
    pub bucket: String,
    #[serde(default = "default_region")]
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: String,
    pub public_url_base: Option<String>,
    pub imagor_secret: Option<String>,
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Prod,
}

const fn default_port() -> u16 {
    3000
}

const fn default_metrics_port() -> u16 {
    9091
}

fn default_public_url_base() -> String {
    format!("http://localhost:{}", default_port())
}

const fn default_file_upload_limit() -> usize {
    5 * 1024 * 1024 // 5 MB
}

const fn default_environment() -> Environment {
    Environment::Dev
}

#[derive(Clone, Deserialize, Debug)]
#[allow(dead_code)]
pub struct ResendConfig {
    pub api_key: String,
    pub from_email: String,
}

#[derive(Clone, Deserialize, Debug)]
#[allow(dead_code)]
pub struct MaidConfig {
    #[serde(default = "default_maid_port")]
    pub port: u16,
    #[serde(default = "default_health_check_timeout_ms")]
    pub health_check_timeout_ms: u64,
    #[serde(default = "default_wait_between_tasks_ms")]
    pub wait_between_tasks_ms: u64,
    #[serde(default = "default_task_timeout_ms")]
    pub task_timeout_ms: u64,
}

const fn default_maid_port() -> u16 {
    3003
}

const fn default_health_check_timeout_ms() -> u64 {
    10_000
}

const fn default_wait_between_tasks_ms() -> u64 {
    1_000
}

const fn default_task_timeout_ms() -> u64 {
    15_000
}

#[derive(Clone, Deserialize, Debug, Default)]
pub struct BannerConfig {
    pub message: String,
    pub kind: String,
    pub enabled: bool,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EmailConfig {
    Resend(ResendConfig),
    Mock,
}

#[derive(Clone, Deserialize, Debug)]
pub struct LexurgyConfig {
    pub url: String,
    pub api_key: String,
}

const fn default_grammar_lexurgy_concurrency() -> usize {
    4
}
const fn default_grammar_table_budget_ms() -> u64 {
    2_000
}
const fn default_grammar_section_budget_ms() -> u64 {
    4_000
}
const fn default_grammar_full_page_budget_ms() -> u64 {
    20_000
}
const fn default_grammar_cache_ttl_seconds() -> i64 {
    90 * 24 * 60 * 60
}
const fn default_grammar_runner_version() -> i32 {
    1
}

#[derive(Clone, Deserialize, Debug)]
pub struct GrammarConfig {
    #[serde(default = "default_grammar_lexurgy_concurrency")]
    pub lexurgy_concurrency: usize,
    #[serde(default = "default_grammar_table_budget_ms")]
    pub table_budget_ms: u64,
    #[serde(default = "default_grammar_section_budget_ms")]
    pub section_budget_ms: u64,
    #[serde(default = "default_grammar_full_page_budget_ms")]
    pub full_page_budget_ms: u64,
    #[serde(default = "default_grammar_cache_ttl_seconds")]
    pub cache_ttl_seconds: i64,
    #[serde(default = "default_grammar_runner_version")]
    pub runner_version: i32,
}

impl Default for GrammarConfig {
    fn default() -> Self {
        Self {
            lexurgy_concurrency: default_grammar_lexurgy_concurrency(),
            table_budget_ms: default_grammar_table_budget_ms(),
            section_budget_ms: default_grammar_section_budget_ms(),
            full_page_budget_ms: default_grammar_full_page_budget_ms(),
            cache_ttl_seconds: default_grammar_cache_ttl_seconds(),
            runner_version: default_grammar_runner_version(),
        }
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub s3: S3Config,
    #[allow(dead_code)]
    pub email: EmailConfig,
    #[allow(dead_code)]
    pub maid: MaidConfig,
    #[serde(default = "default_file_upload_limit")]
    #[allow(dead_code)]
    pub file_upload_limit_bytes: usize,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    #[serde(default = "default_public_url_base")]
    pub public_url_base: String,
    #[serde(default = "default_environment")]
    pub environment: Environment,
    #[serde(default)]
    pub banner: BannerConfig,
    pub lexurgy: LexurgyConfig,
    #[serde(default)]
    pub grammar: GrammarConfig,
}

impl AppConfig {
    pub fn url(&self, path: &str) -> String {
        if self.public_url_base.ends_with('/') {
            format!("{}{}", self.public_url_base, path.trim_start_matches('/'))
        } else {
            format!("{}/{}", self.public_url_base, path.trim_start_matches('/'))
        }
    }
}

pub static CONFIG: LazyLock<AppConfig> = LazyLock::new(|| {
    #[cfg(test)]
    {
        // should match docker-compose.db.test.yml,
        // docker-compose.minio.test.yml,
        // and the justfile
        AppConfig {
            database_url: "postgres://user_test:password@localhost:2435/axismundi_test".to_string(),
            s3: S3Config {
                bucket: "axismundi-test".to_string(),
                region: "us-east-1".to_string(),
                access_key: "minioadmin_test".to_string(),
                secret_key: "minioadmin123_test".to_string(),
                endpoint: "http://localhost:7000".to_string(),
                public_url_base: Some("http://localhost:7888".to_string()),
                imagor_secret: Some("change-me-in-production".to_string()),
            },
            maid: MaidConfig {
                health_check_timeout_ms: default_health_check_timeout_ms(),
                wait_between_tasks_ms: default_wait_between_tasks_ms(),
                task_timeout_ms: default_task_timeout_ms(),
                port: 3003,
            },
            email: EmailConfig::Mock,
            file_upload_limit_bytes: default_file_upload_limit(),
            port: 3001,
            metrics_port: default_metrics_port(),
            public_url_base: "http://localhost:3001".to_string(),
            banner: BannerConfig {
                message: "This is a test banner".to_string(),
                kind: "info".to_string(),
                enabled: false,
            },
            lexurgy: LexurgyConfig {
                url: "http://localhost:4000".to_string(),
                api_key: "change-me-in-production".to_string(),
            },
            grammar: GrammarConfig::default(),
            // TODO: seems bad!
            environment: Environment::Dev,
        }
    }
    #[cfg(not(test))]
    {
        let path = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "./config.json".to_string());
        let path = std::path::Path::new(&path);
        let config_str = std::fs::read_to_string(path)
            .expect("No config file found; please either create a config.json or pass a path to an existing one as the first argument.");
        match serde_json::from_str(&config_str) {
            Ok(config) => config,
            Err(e) => panic!("Failed to parse config file: {e}"),
        }
    }
});
