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
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Prod,
}

const fn default_port() -> u16 {
    3000
}

const fn default_file_upload_limit() -> usize {
    5 * 1024 * 1024 // 5 MB
}

const fn default_environment() -> Environment {
    Environment::Dev
}

#[derive(Clone, Deserialize, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub s3: S3Config,

    #[serde(default = "default_file_upload_limit")]
    pub file_upload_limit_bytes: usize,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_environment")]
    pub environment: Environment,
}

pub static CONFIG: LazyLock<AppConfig> = LazyLock::new(|| {
    if cfg!(test) {
        // should match docker-compose.db.test.yml,
        // docker-compose.minio.test.yml,
        // and the justfile
        return AppConfig {
            database_url: "postgres://user_test:password@localhost:2435/axismundi_test".to_string(),
            s3: S3Config {
                bucket: "axismundi-test".to_string(),
                region: "us-east-1".to_string(),
                access_key: "minioadmin_test".to_string(),
                secret_key: "minioadmin123_test".to_string(),
                endpoint: "http://localhost:7000".to_string(),
                public_url_base: Some("http://localhost:6060/axismundi-test".to_string()),
            },
            file_upload_limit_bytes: default_file_upload_limit(),
            port: 3001,
            // TODO: seems bad!
            environment: Environment::Dev,
        };
    }
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.json".to_string());
    let config_str = std::fs::read_to_string(path)
        .expect("No config file found; please either create a config.json or pass a path to an existing one as the first argument.");
    match serde_json::from_str(&config_str) {
        Ok(config) => config,
        Err(e) => panic!("Failed to parse config file: {e}"),
    }
});
