use crate::{
    config::CONFIG,
    err::{AppResult, internal_error},
};
use image::{ImageReader, codecs::webp::WebPEncoder};
use s3::{Bucket, Region, creds::Credentials};
use std::sync::LazyLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: Box<Bucket>,
    pub public_url_base: Option<String>,
}

const PFP_SIZE: u32 = 256;

pub static S3: LazyLock<S3Config> = LazyLock::new(|| {
    let config = &CONFIG.s3;

    let region = Region::Custom {
        region: config.region.clone(),
        endpoint: config.endpoint.clone(),
    };

    let credentials = match Credentials::new(
        Some(&config.access_key),
        Some(&config.secret_key),
        None,
        None,
        None,
    ) {
        Ok(creds) => creds,
        Err(e) => panic!("Failed to create S3 credentials: {e}"),
    };

    let bucket = match Bucket::new(&config.bucket, region, credentials) {
        Ok(bucket) => bucket.with_path_style(),
        Err(e) => panic!("Failed to initialize S3 bucket: {e}"),
    };

    S3Config {
        bucket,
        public_url_base: config.public_url_base.clone(),
    }
});

impl S3Config {
    pub async fn upload_profile_picture(
        &self,
        user_id: Uuid,
        file_data: &[u8],
    ) -> AppResult<String> {
        // TODO: like half of this should probably be done in a different function,
        // if not moved to a different thread w/ resource limits
        let reader = ImageReader::new(std::io::Cursor::new(file_data))
            .with_guessed_format()?
            .decode()?;

        let image =
            reader.resize_to_fill(PFP_SIZE, PFP_SIZE, image::imageops::FilterType::Lanczos3);

        // we convert all images to webp; it's well-supported and efficient
        let mut image_data = Vec::new();

        let encoder = WebPEncoder::new_lossless(&mut image_data);

        encoder.encode(
            &image.into_rgba8(),
            PFP_SIZE,
            PFP_SIZE,
            image::ExtendedColorType::Rgba8,
        )?;

        let object_key = format!("profiles/{user_id}/avatar");

        let response = self
            .bucket
            .put_object_with_content_type(&object_key, &image_data, "image/webp")
            .await
            .map_err(|e| internal_error(format!("Failed to upload to S3: {e}")))?;

        if response.status_code() == 200 {
            Ok(object_key)
        } else {
            Err(internal_error(format!(
                "S3 upload failed with status: {}",
                response.status_code()
            )))
        }
    }

    pub fn get_object_url(&self, object_key: &str) -> String {
        if let Some(base_url) = &self.public_url_base {
            format!("{}/{}", base_url.trim_end_matches('/'), object_key)
        } else {
            // Default to bucket endpoint URL construction
            format!(
                "{}/{}/{}",
                self.bucket.url(),
                self.bucket.name(),
                object_key
            )
        }
    }
}
