use std::env;
use anyhow::{Result, anyhow};
use s3::{Bucket, Region, creds::Credentials};

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: Box<Bucket>,
    pub public_url_base: Option<String>,
}

impl S3Config {
    pub fn new() -> Result<Self> {
        let bucket_name = env::var("S3_BUCKET_NAME")
            .map_err(|_| anyhow!("S3_BUCKET_NAME environment variable is required"))?;
        
        let region_name = env::var("S3_REGION")
            .unwrap_or_else(|_| "us-east-1".to_string());
        
        let endpoint = env::var("S3_ENDPOINT")
            .map_err(|_| anyhow!("S3_ENDPOINT environment variable is required"))?;
        
        let public_url_base = env::var("S3_PUBLIC_URL_BASE").ok();
        
        let region = Region::Custom {
            region: region_name,
            endpoint,
        };

        let credentials = Credentials::from_env()
            .map_err(|e| anyhow!("Failed to load S3 credentials from environment: {}", e))?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| anyhow!("Failed to create S3 bucket: {}", e))?;

        Ok(Self { bucket, public_url_base })
    }

    pub async fn upload_profile_picture(&self, user_id: i32, file_data: &[u8], content_type: &str) -> Result<String> {
        let object_key = format!("profiles/{}/avatar", user_id);
        
        let response = self.bucket
            .put_object_with_content_type(&object_key, file_data, content_type)
            .await
            .map_err(|e| anyhow!("Failed to upload to S3: {}", e))?;

        if response.status_code() == 200 {
            Ok(object_key)
        } else {
            Err(anyhow!("S3 upload failed with status: {}", response.status_code()))
        }
    }

    pub fn get_profile_picture_url(&self, object_key: &str) -> String {
        if let Some(base_url) = &self.public_url_base {
            format!("{}/{}", base_url.trim_end_matches('/'), object_key)
        } else {
            // Default to bucket endpoint URL construction
            format!("{}/{}/{}", 
                    self.bucket.url(), 
                    self.bucket.name(),
                    object_key)
        }
    }
}