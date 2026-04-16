use crate::{
    config::CONFIG,
    err::{AppResult, internal_error},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use s3::{Bucket, Region, creds::Credentials};
use std::sync::LazyLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct S3Config {
    pub bucket: Box<Bucket>,
    pub imagor_base_url: String,
    pub imagor_secret: String,
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

    let imagor_base_url = config
        .public_url_base
        .as_ref()
        .expect("public_url_base must be set for S3 config")
        .clone();
    let imagor_secret = config
        .imagor_secret
        .as_ref()
        .expect("imagor_secret must be set for S3 config")
        .clone();

    S3Config {
        bucket,
        imagor_base_url,
        imagor_secret,
    }
});

fn sign_imagor_path(path: &str, secret: &str) -> String {
    let hash = hmacsha1::hmac_sha1(secret.as_bytes(), path.as_bytes());
    URL_SAFE.encode(hash)
}

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
        let (upload_data, upload_type): (Vec<u8>, &str) = if content_type == "image/gif" {
            (
                super::images::convert_gif_to_animated_webp(file_data)?,
                "image/webp",
            )
        } else {
            (file_data.to_vec(), content_type)
        };

        let extension = match upload_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/avif" => "avif",
            _ => return Err(internal_error("unsupported image type")),
        };

        let filename = format!("profile-{}-{}.{}", user_id, Uuid::new_v4(), extension);
        self.upload_image_original(&filename, &upload_data, upload_type)
            .await
    }

    // /// gets the url for an original image stored in minio
    // pub fn get_original_url(&self, filename: &str) -> String {
    //     format!(
    //         "{}/originals/{}",
    //         self.public_url_base.trim_end_matches('/'),
    //         filename
    //     )
    // }

    /// generates a signed imagor url with smart cropping and webp output
    pub fn get_image_url_smart(&self, filename: &str, width: u32, height: u32) -> String {
        let path = format!(
            "{}x{}/smart/filters:format(webp)/originals/{}",
            width, height, filename
        );
        let hash = sign_imagor_path(&path, &self.imagor_secret);
        format!(
            "{}/{}/{}",
            self.imagor_base_url.trim_end_matches('/'),
            hash,
            path
        )
    }

    pub fn get_profile_picture_url(&self, filename: &str) -> String {
        self.get_image_url_smart(filename, 256, 256)
    }

    pub fn get_banner_url(&self, filename: &str) -> String {
        self.get_image_url_smart(filename, 800, 200)
    }

    pub async fn upload_banner(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        file_data: &[u8],
        content_type: &str,
    ) -> AppResult<String> {
        let (upload_data, upload_type): (Vec<u8>, &str) = if content_type == "image/gif" {
            (
                super::images::convert_gif_to_animated_webp(file_data)?,
                "image/webp",
            )
        } else {
            (file_data.to_vec(), content_type)
        };

        let extension = match upload_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/avif" => "avif",
            _ => return Err(internal_error("unsupported image type")),
        };

        let filename = format!(
            "banner-{}-{}-{}.{}",
            entity_type,
            entity_id,
            Uuid::new_v4(),
            extension
        );
        self.upload_image_original(&filename, &upload_data, upload_type)
            .await
    }
}
