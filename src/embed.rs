use axum::response::Html;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

use crate::{
    config::CONFIG,
    err::{AppError, AppResult, bad_request},
    model::{
        language_families::LanguageFamilyRepository,
        languages::LanguageRepository,
        translatable::TranslatableRepository,
        translations::TranslationRepository,
        users::{User, UserRepository},
        word_classes::WordClassRepository,
    },
    util::AppState,
};

// https://oembed.com/#section2.2 Consumer Request
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct OEmbedRequest {
    /// The URL to retrieve embedding information for
    pub url: String,
    /// Maximum width of the embedded resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxwidth: Option<u32>,
    /// Maximum height of the embedded resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxheight: Option<u32>,
    /// Response format (json or xml)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

// https://oembed.com/#section2.3 Provider Response
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
#[allow(dead_code)]
pub enum OEmbedResponse {
    Photo {
        /// oEmbed version number (must be 1.0)
        version: String,
        /// The source URL of the image
        url: String,
        /// Width in pixels
        width: u32,
        /// Height in pixels
        height: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_age: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_height: Option<u32>,
    },
    Video {
        version: String,
        /// The HTML required to embed the video
        html: String,
        /// Width in pixels
        width: u32,
        /// Height in pixels
        height: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_age: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_height: Option<u32>,
    },
    Link {
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_age: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_height: Option<u32>,
    },
    Rich {
        version: String,
        /// The HTML required to display the resource
        html: String,
        /// Width in pixels
        width: u32,
        /// Height in pixels
        height: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_age: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail_height: Option<u32>,
    },
}

pub async fn get_oembed(state: AppState, request: &OEmbedRequest) -> AppResult<OEmbedResponse> {
    if Some("xml".to_string()) == request.format {
        return Err(AppError::new(
            "xml is not supported".to_string(),
            StatusCode::from_u16(501).unwrap(),
        ));
    }

    let url = url::Url::parse(&request.url).map_err(|_| bad_request("could not parse url"))?;

    let mut segments = url
        .path_segments()
        .ok_or_else(|| bad_request("url has no path segments"))?;

    match segments.next() {
        Some("languages") => {
            let languages = LanguageRepository::new(state.clone());
            let code = segments
                .next()
                .ok_or_else(|| bad_request("missing language slug"))?;
            let language = languages.find_by_code(code).await?;

            // Check if this is a word-class sub-path
            if let Some("word-classes") = segments.next() {
                let abbreviation = segments
                    .next()
                    .ok_or_else(|| bad_request("missing word class abbreviation"))?;
                let word_classes = WordClassRepository::new(state.clone());
                let word_class = word_classes
                    .find_by_abbreviation(&language.id, abbreviation)
                    .await?;
                let users = UserRepository::new(state.clone());
                let author = users.find_by_id(word_class.created_by).await?;
                let author_name = author.name().to_string();
                let author_url = format!("{}/users/{}", &CONFIG.public_url_base, author.username);

                return Ok(OEmbedResponse::Link {
                    version: "1.0".to_string(),
                    title: Some(format!(
                        "{} ({}.)",
                        word_class.name, word_class.abbreviation
                    )),
                    author_name: Some(author_name),
                    author_url: Some(author_url),
                    provider_name: Some("Axismundi".to_string()),
                    provider_url: Some(CONFIG.public_url_base.clone()),
                    cache_age: None,
                    thumbnail_url: None,
                    thumbnail_width: None,
                    thumbnail_height: None,
                });
            }

            let author = languages.find_owner(language.id).await?;
            let author_name = author.name().to_string();
            let author_url = format!("{}/users/{}", &CONFIG.public_url_base, author.username);

            Ok(OEmbedResponse::Link {
                version: "1.0".to_string(),
                title: Some(language.name),
                author_name: Some(author_name),
                author_url: Some(author_url),
                provider_name: Some("Axismundi".to_string()),
                provider_url: Some(CONFIG.public_url_base.clone()),
                cache_age: None,
                thumbnail_url: None,
                thumbnail_width: None,
                thumbnail_height: None,
            })
        }
        Some("language-families") => {
            let families = LanguageFamilyRepository::new(state.clone());
            let code = segments
                .next()
                .ok_or_else(|| bad_request("missing language family slug"))?;
            let family = families.find_by_code(code).await?;
            let author = families.find_owner(family.id).await?;
            let author_name = author.name().to_string();
            let author_url = format!("{}/users/{}", &CONFIG.public_url_base, author.username);

            Ok(OEmbedResponse::Link {
                version: "1.0".to_string(),
                title: Some(family.name),
                author_name: Some(author_name),
                author_url: Some(author_url),
                provider_name: Some("Axismundi".to_string()),
                provider_url: Some(CONFIG.public_url_base.clone()),
                cache_age: None,
                thumbnail_url: None,
                thumbnail_width: None,
                thumbnail_height: None,
            })
        }
        Some("translatable") => {
            let translatables = TranslatableRepository::new(state.clone());
            let slug = segments
                .next()
                .ok_or_else(|| bad_request("missing translatable slug"))?;
            let translatable = translatables.find_by_slug(slug).await?;

            // Check if this is a translation sub-path: /translatable/{slug}/translation/{code}
            if let Some("translation") = segments.next() {
                let code = segments
                    .next()
                    .ok_or_else(|| bad_request("missing language code"))?;
                let languages = LanguageRepository::new(state.clone());
                let language = languages.find_by_code(code).await?;
                let translations = TranslationRepository::new(state.clone());
                let translation = translations
                    .find_by_translatable_and_language(translatable.id, language.id)
                    .await?;
                let users = UserRepository::new(state.clone());
                let author = users.find_by_id(translation.created_by).await?;
                let author_name = author.name().to_string();
                let author_url = format!("{}/users/{}", &CONFIG.public_url_base, author.username);

                return Ok(OEmbedResponse::Link {
                    version: "1.0".to_string(),
                    title: Some(format!(
                        "{} ({} translation)",
                        translatable.title, language.name
                    )),
                    author_name: Some(author_name),
                    author_url: Some(author_url),
                    provider_name: Some("Axismundi".to_string()),
                    provider_url: Some(CONFIG.public_url_base.clone()),
                    cache_age: None,
                    thumbnail_url: None,
                    thumbnail_width: None,
                    thumbnail_height: None,
                });
            }

            let users = UserRepository::new(state.clone());
            let author = users.find_by_id(translatable.created_by).await?;
            let author_name = author.name().to_string();
            let author_url = format!("{}/users/{}", &CONFIG.public_url_base, author.username);

            Ok(OEmbedResponse::Link {
                version: "1.0".to_string(),
                title: Some(translatable.title),
                author_name: Some(author_name),
                author_url: Some(author_url),
                provider_name: Some("Axismundi".to_string()),
                provider_url: Some(CONFIG.public_url_base.clone()),
                cache_age: None,
                thumbnail_url: None,
                thumbnail_width: None,
                thumbnail_height: None,
            })
        }
        Some("users") => {
            let users = UserRepository::new(state.clone());
            let username = segments
                .next()
                .ok_or_else(|| bad_request("missing username"))?;
            let user = users.find_by_username(username).await?;
            let author_name = user.name().to_string();
            let author_url = format!("{}/users/{}", &CONFIG.public_url_base, user.username);

            Ok(OEmbedResponse::Link {
                version: "1.0".to_string(),
                title: Some(author_name.clone()),
                author_name: Some(author_name),
                author_url: Some(author_url),
                provider_name: Some("Axismundi".to_string()),
                provider_url: Some(CONFIG.public_url_base.clone()),
                cache_age: None,
                thumbnail_url: None,
                thumbnail_width: None,
                thumbnail_height: None,
            })
        }
        _ => {
            // generic response for unsupported urls
            Ok(OEmbedResponse::Link {
                version: "1.0".to_string(),
                title: Some("Axismundi".to_string()),
                author_name: None,
                author_url: None,
                provider_name: Some("Axismundi".to_string()),
                provider_url: Some(CONFIG.public_url_base.clone()),
                cache_age: None,
                thumbnail_url: None,
                thumbnail_width: None,
                thumbnail_height: None,
            })
        }
    }
}

pub const MAX_CONTENT_LEN: usize = 200;

pub fn truncate_description(s: &str) -> String {
    if s.len() > MAX_CONTENT_LEN {
        let truncated = s
            .char_indices()
            .take_while(|(i, _)| *i < MAX_CONTENT_LEN - 1)
            .last()
            .map_or("", |(i, c)| &s[..i + c.len_utf8()]);
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}

#[allow(dead_code)]
pub struct GenericEmbed {
    pub title: String,
    pub description: String,
    pub author: Option<User>,
    pub color: Option<String>,
    pub url: String,
    pub image: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum EmbedTarget {
    Discord,
}

pub async fn render_embed(target: EmbedTarget, embed: GenericEmbed) -> Html<String> {
    fn base_embed_html(head: &str, body: &str) -> Html<String> {
        Html(format!(
            "<!DOCTYPE html><html><head>{}</head><body>{}</body></html>",
            head, body
        ))
    }
    fn render_error(err: AppError) -> Html<String> {
        let status = err.status_code.canonical_reason().unwrap_or("Error");
        let title = format!("{} {}", err.status_code, status);
        let description = err.message;
        let headers = format!(
            "<meta property=og:title content=\"{title}\"/><meta property=og:description content=\"{description}\">"
        );
        base_embed_html(&headers, "")
    }
    #[allow(clippy::unnecessary_wraps)]
    fn inner(_target: EmbedTarget, embed: GenericEmbed) -> Result<Html<String>, AppError> {
        let mut headers = format!(
            "<meta property=og:title content=\"{}\"/><meta property=og:description content=\"{}\">",
            embed.title, embed.description
        );

        headers.push_str(
            "
            <meta property=\"og:site_name\" content=\"Axismundi\" />
            <link rel=\"icon\" type=\"image/svg+xml\" href=\"/assets/favicon.svg\">
            ",
        );

        if let Some(image) = embed.image {
            write!(headers, "<meta property=og:image content=\"{image}\">")?;
        }

        if let Some(color) = embed.color {
            write!(headers, "<meta name=theme-color content=\"{color}\">")?;
        }

        Ok(base_embed_html(&headers, ""))
    }

    inner(target, embed).unwrap_or_else(render_error)
}
