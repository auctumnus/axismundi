use crate::{
    config::CONFIG,
    err::{AppResult, internal_error},
};
use s3::{Bucket, Region, creds::Credentials};
use std::sync::LazyLock;
use uuid::Uuid;
use thumbor::Server;

#[derive(Clone)]
pub struct S3Config {
    pub bucket: Box<Bucket>,
    pub public_url_base: String,
    pub thumbor_server: Server,
}

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

    let thumbor_url = config.public_url_base.as_ref().expect("public_url_base must be set for S3 config").clone();
    let thumbor_security_key = config.thumbor_security_key.as_ref().expect("thumbor_security_key must be set for S3 config").clone();

    let thumbor_server = thumbor::Server::new(&thumbor_url, &thumbor_security_key).unwrap();

    S3Config {
        bucket,
        public_url_base: thumbor_url,
        thumbor_server,
    }
});

impl S3Config {
    /// uploads an image to the originals/ prefix in minio
    /// returns the filename (without the originals/ prefix)
    pub async fn upload_image_original(
        &self,
        filename: &str,
        file_data: &[u8],
        content_type: &str,
    ) -> AppResult<String> {
        let object_key = format!("originals/{}", filename);

        let response = self
            .bucket
            .put_object_with_content_type(&object_key, file_data, content_type)
            .await
            .map_err(|e| internal_error(format!("failed to upload to s3: {e}")))?;

        if response.status_code() == 200 {
            Ok(filename.to_string())
        } else {
            Err(internal_error(format!(
                "s3 upload failed with status: {}",
                response.status_code()
            )))
        }
    }

    /// uploads a profile picture to minio
    /// returns the filename
    pub async fn upload_profile_picture(
        &self,
        user_id: Uuid,
        file_data: &[u8],
        content_type: &str,
    ) -> AppResult<String> {
        // determine file extension from content type
        let extension = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/avif" => "avif",
            _ => return Err(internal_error("unsupported image type")),
        };

        let filename = format!("profile-{}.{}", user_id, extension);
        self.upload_image_original(&filename, file_data, content_type).await
    }

    /// gets the url for an original image stored in minio
    pub fn get_original_url(&self, filename: &str) -> String {
        format!("{}/originals/{}", self.public_url_base.trim_end_matches('/'), filename)
    }

    /// generates a thumbor url for an image with the given dimensions
    /// if thumbor isn't configured, falls back to the original url
    /// if a security key is configured, generates a signed url
    pub fn get_thumbor_url(&self, filename: &str, width: u32, height: u32) -> String {
        let image_url = format!("originals/{}", filename);
        self.thumbor_server.endpoint_builder().resize((width as i32, height as i32)).build().to_url(&image_url)
    }

    /// generates a thumbor url with smart cropping (face/feature detection)
    /// if a security key is configured, generates a signed url
    pub fn get_thumbor_url_smart(&self, filename: &str, width: u32, height: u32) -> String {
        let image_url = format!("originals/{}", filename);
        self.thumbor_server.endpoint_builder().resize((width as i32, height as i32)).smart(true).build().to_url(&image_url)
    }

    /// generates a thumbor url that fits the image inside the dimensions (maintains aspect ratio)
    /// if a security key is configured, generates a signed url
    pub fn get_thumbor_url_fit(&self, filename: &str, width: u32, height: u32) -> String {
        let image_url = format!("originals/{}", filename);
        self.thumbor_server.endpoint_builder().resize((width as i32, height as i32)).fit_in(thumbor::endpoint::FitIn::Default).build().to_url(&image_url)
    }

    // legacy helper that returns a thumbor url for profile pictures
    // uses smart crop since it's a profile picture
    pub fn get_profile_picture_url(&self, filename: &str) -> String {
        self.get_thumbor_url_smart(filename, 256, 256)
    }
}
