//! Commands for community content: translatables, translations, quotations, and news.
#![allow(private_interfaces)]

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde_json::json;

use super::{ApiRequest, path_segment};

#[derive(Debug, Subcommand)]
pub(crate) enum ContentCommand {
    /// Manage source texts that can be translated into community languages.
    Translatable {
        #[command(subcommand)]
        command: TranslatableCommand,
    },
    /// Manage a language's translation of a source text.
    Translation {
        #[command(subcommand)]
        command: TranslationCommand,
    },
    /// Manage quotations that connect a translation to a word definition.
    Quotation {
        #[command(subcommand)]
        command: QuotationCommand,
    },
    /// Manage news articles.
    News {
        #[command(subcommand)]
        command: NewsCommand,
    },
}

pub(crate) fn content_command_request(command: ContentCommand) -> Result<ApiRequest> {
    match command {
        ContentCommand::Translatable { command } => translatable_request(command),
        ContentCommand::Translation { command } => translation_request(command),
        ContentCommand::Quotation { command } => quotation_request(command),
        ContentCommand::News { command } => news_request(command),
    }
}

#[derive(Debug, Subcommand)]
pub(super) enum TranslatableCommand {
    /// List published source texts.
    List,
    /// Fetch a source text by slug.
    #[command(visible_alias = "read")]
    Get(TranslatableSlugArgs),
    /// Create a source text.
    New(TranslatableNewArgs),
    /// Update a source text you created.
    Edit(TranslatableEditArgs),
    /// Delete a source text you created.
    Delete(TranslatableSlugArgs),
}

#[derive(Debug, Args)]
pub(super) struct TranslatableSlugArgs {
    pub(super) slug: String,
}

#[derive(Debug, Args)]
pub(super) struct TranslatableNewArgs {
    #[arg(long)]
    pub(super) title: String,
    #[arg(long)]
    pub(super) english: String,
    #[arg(long)]
    pub(super) source_name: Option<String>,
    /// Canonical URL for the source. Use an empty value to record no URL.
    #[arg(long)]
    pub(super) source_url: Option<String>,
    #[arg(long)]
    pub(super) source_content: Option<String>,
    #[arg(long)]
    pub(super) source_language: Option<String>,
    #[arg(long)]
    pub(super) description: Option<String>,
    /// Create as a staff-only draft rather than publishing immediately.
    #[arg(long)]
    pub(super) draft: bool,
}

#[derive(Debug, Args)]
pub(super) struct TranslatableEditArgs {
    pub(super) slug: String,
    #[arg(long)]
    pub(super) title: Option<String>,
    #[arg(long)]
    pub(super) english: Option<String>,
    #[arg(long)]
    pub(super) source_name: Option<String>,
    #[arg(long)]
    pub(super) source_url: Option<String>,
    #[arg(long)]
    pub(super) source_content: Option<String>,
    #[arg(long)]
    pub(super) source_language: Option<String>,
    #[arg(long)]
    pub(super) description: Option<String>,
}

pub(super) fn translatable_request(command: TranslatableCommand) -> Result<ApiRequest> {
    match command {
        TranslatableCommand::List => Ok(ApiRequest::new("GET", "translatable".to_owned(), None)),
        TranslatableCommand::Get(args) => {
            let slug = path_segment("SLUG", &args.slug)?;
            Ok(ApiRequest::new("GET", format!("translatable/{slug}"), None))
        }
        TranslatableCommand::New(args) => Ok(ApiRequest::new(
            "POST",
            "translatable".to_owned(),
            Some(json!({
                "title": args.title, "english": args.english,
                "source_name": args.source_name, "source_url": args.source_url,
                "source_content": args.source_content, "source_language": args.source_language,
                "description": args.description, "as_draft": args.draft,
            })),
        )),
        TranslatableCommand::Edit(args) => {
            let slug = path_segment("SLUG", &args.slug)?;
            Ok(ApiRequest::new(
                "PUT",
                format!("translatable/{slug}"),
                Some(json!({
                    "title": args.title, "english": args.english,
                    "source_name": args.source_name, "source_url": args.source_url,
                    "source_content": args.source_content, "source_language": args.source_language,
                    "description": args.description,
                })),
            ))
        }
        TranslatableCommand::Delete(args) => {
            let slug = path_segment("SLUG", &args.slug)?;
            Ok(ApiRequest::new(
                "DELETE",
                format!("translatable/{slug}"),
                None,
            ))
        }
    }
}

#[derive(Debug, Subcommand)]
pub(super) enum TranslationCommand {
    /// List translations for a source text or all translations in a language.
    List(TranslationListArgs),
    /// Fetch a translation by source-text slug and language code.
    #[command(visible_alias = "read")]
    Get(TranslationLocatorArgs),
    /// Create a translation.
    New(TranslationNewArgs),
    /// Update a translation.
    Edit(TranslationEditArgs),
    /// Delete a translation.
    Delete(TranslationLocatorArgs),
}

#[derive(Debug, Args)]
pub(super) struct TranslationListArgs {
    /// Source-text slug. Mutually exclusive with --in.
    #[arg(long, conflicts_with = "language")]
    pub(super) translatable: Option<String>,
    /// Language code. With no --translatable, lists all translations in this language.
    #[arg(long = "in", conflicts_with = "translatable")]
    pub(super) language: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct TranslationLocatorArgs {
    #[arg(long)]
    pub(super) translatable: String,
    #[arg(long = "in", env = "AXM_DEFAULT_LANGUAGE")]
    pub(super) language: String,
}

#[derive(Debug, Args)]
pub(super) struct TranslationNewArgs {
    #[command(flatten)]
    pub(super) locator: TranslationLocatorArgs,
    #[arg(long)]
    pub(super) text: String,
    #[arg(long)]
    pub(super) title: Option<String>,
    #[arg(long)]
    pub(super) ipa: Option<String>,
    #[arg(long)]
    pub(super) gloss: Option<String>,
    #[arg(long)]
    pub(super) notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct TranslationEditArgs {
    #[command(flatten)]
    pub(super) locator: TranslationLocatorArgs,
    #[arg(long)]
    pub(super) text: Option<String>,
    #[arg(long)]
    pub(super) title: Option<String>,
    #[arg(long)]
    pub(super) ipa: Option<String>,
    #[arg(long)]
    pub(super) gloss: Option<String>,
    #[arg(long)]
    pub(super) notes: Option<String>,
}

pub(super) fn translation_request(command: TranslationCommand) -> Result<ApiRequest> {
    match command {
        TranslationCommand::List(args) => match (args.translatable, args.language) {
            (Some(translatable), None) => {
                let translatable = path_segment("--translatable", &translatable)?;
                Ok(ApiRequest::new(
                    "GET",
                    format!("translatable/{translatable}/translations"),
                    None,
                ))
            }
            (None, Some(language)) => {
                let language = path_segment("--in", &language)?;
                Ok(ApiRequest::new(
                    "GET",
                    format!("languages/{language}/translations"),
                    None,
                ))
            }
            (None, None) => bail!("translation list requires --translatable or --in"),
            (Some(_), Some(_)) => unreachable!("clap enforces these arguments conflict"),
        },
        TranslationCommand::Get(args) => Ok(ApiRequest::new("GET", translation_path(&args)?, None)),
        TranslationCommand::New(args) => Ok(ApiRequest::new(
            "POST",
            translation_path(&args.locator)?,
            Some(json!({
                "translated_text": args.text, "translated_title": args.title,
                "ipa": args.ipa, "gloss": args.gloss, "notes": args.notes,
            })),
        )),
        TranslationCommand::Edit(args) => Ok(ApiRequest::new(
            "PUT",
            translation_path(&args.locator)?,
            Some(json!({
                "translated_text": args.text, "translated_title": args.title,
                "ipa": args.ipa, "gloss": args.gloss, "notes": args.notes,
            })),
        )),
        TranslationCommand::Delete(args) => {
            Ok(ApiRequest::new("DELETE", translation_path(&args)?, None))
        }
    }
}

fn translation_path(args: &TranslationLocatorArgs) -> Result<String> {
    let translatable = path_segment("--translatable", &args.translatable)?;
    let language = path_segment("--in", &args.language)?;
    Ok(format!(
        "translatable/{translatable}/translations/{language}"
    ))
}

#[derive(Debug, Subcommand)]
pub(super) enum QuotationCommand {
    /// List quotations attached to a translation.
    List(QuotationTranslationArgs),
    /// Fetch a quotation attached to a translation.
    #[command(visible_alias = "read")]
    Get(QuotationLocatorArgs),
    /// Create a quotation attached to a translation and a word definition.
    New(QuotationNewArgs),
    /// Update a quotation.
    Edit(QuotationEditArgs),
    /// Delete a quotation.
    Delete(QuotationLocatorArgs),
}

#[derive(Debug, Args)]
pub(super) struct QuotationTranslationArgs {
    #[arg(long)]
    pub(super) translatable: String,
    #[arg(long = "in", env = "AXM_DEFAULT_LANGUAGE")]
    pub(super) language: String,
}

#[derive(Debug, Args)]
pub(super) struct QuotationLocatorArgs {
    #[command(flatten)]
    pub(super) translation: QuotationTranslationArgs,
    /// Quotation UUID.
    pub(super) id: String,
}

#[derive(Debug, Args)]
pub(super) struct QuotationNewArgs {
    #[command(flatten)]
    pub(super) translation: QuotationTranslationArgs,
    /// UUID of the definition this quotation illustrates.
    #[arg(long)]
    pub(super) definition: String,
    /// UTF-16 offsets in the translation.
    #[arg(long)]
    pub(super) span_start: i32,
    #[arg(long)]
    pub(super) span_end: i32,
    #[arg(long)]
    pub(super) highlight_start: Option<i32>,
    #[arg(long)]
    pub(super) highlight_end: Option<i32>,
    #[arg(long)]
    pub(super) notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct QuotationEditArgs {
    #[command(flatten)]
    pub(super) locator: QuotationLocatorArgs,
    #[arg(long)]
    pub(super) span_start: Option<i32>,
    #[arg(long)]
    pub(super) span_end: Option<i32>,
    #[arg(long, conflicts_with = "clear_highlight_start")]
    pub(super) highlight_start: Option<i32>,
    /// Clear the start of the optional highlight range.
    #[arg(long)]
    pub(super) clear_highlight_start: bool,
    #[arg(long, conflicts_with = "clear_highlight_end")]
    pub(super) highlight_end: Option<i32>,
    /// Clear the end of the optional highlight range.
    #[arg(long)]
    pub(super) clear_highlight_end: bool,
    #[arg(long)]
    pub(super) notes: Option<String>,
}

pub(super) fn quotation_request(command: QuotationCommand) -> Result<ApiRequest> {
    match command {
        QuotationCommand::List(args) => Ok(ApiRequest::new(
            "GET",
            quotation_collection_path(&args)?,
            None,
        )),
        QuotationCommand::Get(args) => Ok(ApiRequest::new("GET", quotation_path(&args)?, None)),
        QuotationCommand::New(args) => {
            let definition = path_segment("--definition", &args.definition)?;
            Ok(ApiRequest::new(
                "POST",
                quotation_collection_path(&args.translation)?,
                Some(json!({
                    "definition": definition, "span_start": args.span_start, "span_end": args.span_end,
                    "highlight_start": args.highlight_start, "highlight_end": args.highlight_end,
                    "notes": args.notes.unwrap_or_default(),
                })),
            ))
        }
        QuotationCommand::Edit(args) => {
            let highlight_start = optional_highlight(
                args.highlight_start,
                args.clear_highlight_start,
                "--highlight-start",
            )?;
            let highlight_end = optional_highlight(
                args.highlight_end,
                args.clear_highlight_end,
                "--highlight-end",
            )?;
            let mut body = json!({
                "span_start": args.span_start, "span_end": args.span_end,
                "notes": args.notes,
            });
            // UpdateQuotation treats an omitted highlight differently from an
            // explicit JSON null: null clears the stored boundary.
            if let Some(highlight_start) = highlight_start {
                body["highlight_start"] = json!(highlight_start);
            }
            if let Some(highlight_end) = highlight_end {
                body["highlight_end"] = json!(highlight_end);
            }
            Ok(ApiRequest::new(
                "PUT",
                quotation_path(&args.locator)?,
                Some(body),
            ))
        }
        QuotationCommand::Delete(args) => {
            Ok(ApiRequest::new("DELETE", quotation_path(&args)?, None))
        }
    }
}

/// `None` means an update omitted this field; `Some(None)` clears it.
fn optional_highlight(
    value: Option<i32>,
    clear: bool,
    argument: &str,
) -> Result<Option<Option<i32>>> {
    if value.is_some() && clear {
        bail!("{argument} cannot be used with its corresponding --clear flag");
    }
    Ok(if clear { Some(None) } else { value.map(Some) })
}

fn quotation_collection_path(args: &QuotationTranslationArgs) -> Result<String> {
    let translatable = path_segment("--translatable", &args.translatable)?;
    let language = path_segment("--in", &args.language)?;
    Ok(format!(
        "translatable/{translatable}/translations/{language}/quotations"
    ))
}

fn quotation_path(args: &QuotationLocatorArgs) -> Result<String> {
    let collection = quotation_collection_path(&args.translation)?;
    let id = path_segment("ID", &args.id)?;
    Ok(format!("{collection}/{id}"))
}

#[derive(Debug, Subcommand)]
pub(super) enum NewsCommand {
    /// List published news articles.
    List,
    /// Fetch a news article by slug.
    #[command(visible_alias = "read")]
    Get(NewsSlugArgs),
    /// Create a news article.
    New(NewsNewArgs),
    /// Update a news article.
    Edit(NewsEditArgs),
    /// Delete a news article.
    Delete(NewsSlugArgs),
}

#[derive(Debug, Args)]
pub(super) struct NewsSlugArgs {
    pub(super) slug: String,
}

#[derive(Debug, Args)]
pub(super) struct NewsNewArgs {
    #[arg(long)]
    pub(super) title: String,
    #[arg(long)]
    pub(super) content: String,
    /// Create as a draft rather than publishing immediately.
    #[arg(long)]
    pub(super) draft: bool,
}

#[derive(Debug, Args)]
pub(super) struct NewsEditArgs {
    pub(super) slug: String,
    #[arg(long)]
    pub(super) title: Option<String>,
    #[arg(long)]
    pub(super) content: Option<String>,
}

pub(super) fn news_request(command: NewsCommand) -> Result<ApiRequest> {
    match command {
        NewsCommand::List => Ok(ApiRequest::new("GET", "news".to_owned(), None)),
        NewsCommand::Get(args) => {
            let slug = path_segment("SLUG", &args.slug)?;
            Ok(ApiRequest::new("GET", format!("news/{slug}"), None))
        }
        NewsCommand::New(args) => Ok(ApiRequest::new(
            "POST",
            "news".to_owned(),
            Some(json!({
                "title": args.title, "content": args.content, "as_draft": args.draft,
            })),
        )),
        NewsCommand::Edit(args) => {
            let slug = path_segment("SLUG", &args.slug)?;
            Ok(ApiRequest::new(
                "PUT",
                format!("news/{slug}"),
                Some(json!({ "title": args.title, "content": args.content })),
            ))
        }
        NewsCommand::Delete(args) => {
            let slug = path_segment("SLUG", &args.slug)?;
            Ok(ApiRequest::new("DELETE", format!("news/{slug}"), None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn translatable_new_maps_to_current_payload() {
        let request = translatable_request(TranslatableCommand::New(TranslatableNewArgs {
            title: "A proverb".to_owned(),
            english: "The river remembers.".to_owned(),
            source_name: Some("Field notes".to_owned()),
            source_url: None,
            source_content: None,
            source_language: None,
            description: Some("Collected in 2026".to_owned()),
            draft: true,
        }))
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "translatable");
        assert_eq!(
            request.body,
            Some(json!({
                "title": "A proverb", "english": "The river remembers.", "source_name": "Field notes",
                "source_url": null, "source_content": null, "source_language": null,
                "description": "Collected in 2026", "as_draft": true,
            }))
        );
    }

    #[test]
    fn translation_new_uses_the_nested_translation_route() {
        let request = translation_request(TranslationCommand::New(TranslationNewArgs {
            locator: TranslationLocatorArgs {
                translatable: "a-proverb".to_owned(),
                language: "pas".to_owned(),
            },
            text: "Kok'ebe.".to_owned(),
            title: None,
            ipa: Some("koˈkʼe.be".to_owned()),
            gloss: None,
            notes: None,
        }))
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "translatable/a-proverb/translations/pas");
        assert_eq!(
            request.body,
            Some(json!({
                "translated_text": "Kok'ebe.", "translated_title": null, "ipa": "koˈkʼe.be", "gloss": null, "notes": null,
            }))
        );
    }

    #[test]
    fn quotation_edit_can_clear_a_highlight_boundary() {
        let request = quotation_request(QuotationCommand::Edit(QuotationEditArgs {
            locator: QuotationLocatorArgs {
                translation: QuotationTranslationArgs {
                    translatable: "a-proverb".to_owned(),
                    language: "pas".to_owned(),
                },
                id: "3e2f8a6b-8e5e-4fae-a71a-5906440f2d2d".to_owned(),
            },
            span_start: Some(1),
            span_end: None,
            highlight_start: None,
            clear_highlight_start: true,
            highlight_end: Some(5),
            clear_highlight_end: false,
            notes: Some("Adjusted selection".to_owned()),
        }))
        .unwrap();
        assert_eq!(request.method, "PUT");
        assert_eq!(
            request.path,
            "translatable/a-proverb/translations/pas/quotations/3e2f8a6b-8e5e-4fae-a71a-5906440f2d2d"
        );
        assert_eq!(
            request.body,
            Some(json!({
                "span_start": 1, "span_end": null, "highlight_start": null, "highlight_end": 5, "notes": "Adjusted selection",
            }))
        );
    }

    #[test]
    fn news_delete_rejects_unsafe_path_segment() {
        let error = news_request(NewsCommand::Delete(NewsSlugArgs {
            slug: "news/other".to_owned(),
        }))
        .unwrap_err();
        assert!(error.to_string().contains("single non-empty path segment"));
    }
}
