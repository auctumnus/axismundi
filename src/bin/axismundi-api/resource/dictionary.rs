//! Dictionary-related resource commands.
#![allow(private_interfaces)]

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::{ApiRequest, path_segment};

/// Manage languages.
#[derive(Debug, Subcommand)]
pub(crate) enum LanguageCommand {
    /// List languages.
    List,
    /// Fetch one language by code.
    #[command(visible_alias = "read")]
    Get(LanguageGetArgs),
    /// Create a language.
    New(LanguageNewArgs),
    /// Update a language.
    Edit(LanguageEditArgs),
    /// Delete a language.
    Delete(LanguageDeleteArgs),
}

#[derive(Debug, Args)]
pub(super) struct LanguageGetArgs {
    /// Language code.
    #[arg(long)]
    code: String,
}

#[derive(Debug, Args)]
pub(super) struct LanguageNewArgs {
    /// Unique language code.
    #[arg(long)]
    code: String,
    /// Display name.
    #[arg(long)]
    name: String,
    /// Make the language private. Public is the API default.
    #[arg(long)]
    private: bool,
    /// Markdown description.
    #[arg(long)]
    description: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct LanguageEditArgs {
    /// Existing language code.
    #[arg(long)]
    code: String,
    /// Replacement language code.
    #[arg(long = "new-code")]
    new_code: Option<String>,
    /// Replacement display name.
    #[arg(long)]
    name: Option<String>,
    /// Whether the language is private (true or false).
    #[arg(long, value_name = "BOOL")]
    private: Option<bool>,
    /// Replacement Markdown description.
    #[arg(long)]
    description: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct LanguageDeleteArgs {
    /// Language code.
    #[arg(long)]
    code: String,
}

/// Convert a language command to its API request.
pub(crate) fn language_request(command: LanguageCommand) -> Result<ApiRequest> {
    match command {
        LanguageCommand::List => Ok(ApiRequest::new("GET", "languages".to_owned(), None)),
        LanguageCommand::Get(args) => {
            let code = path_segment("--code", &args.code)?;
            Ok(ApiRequest::new("GET", format!("languages/{code}"), None))
        }
        LanguageCommand::New(args) => {
            let mut body = Map::from_iter([
                ("code".to_owned(), json!(args.code)),
                ("name".to_owned(), json!(args.name)),
            ]);
            if args.private {
                body.insert("private".to_owned(), json!(true));
            }
            insert_optional(&mut body, "description", args.description);
            Ok(ApiRequest::new(
                "POST",
                "languages".to_owned(),
                Some(body.into()),
            ))
        }
        LanguageCommand::Edit(args) => {
            let code = path_segment("--code", &args.code)?;
            let mut body = Map::new();
            insert_optional(&mut body, "code", args.new_code);
            insert_optional(&mut body, "name", args.name);
            if let Some(private) = args.private {
                body.insert("private".to_owned(), json!(private));
            }
            insert_optional(&mut body, "description", args.description);
            Ok(ApiRequest::new(
                "PUT",
                format!("languages/{code}"),
                Some(body.into()),
            ))
        }
        LanguageCommand::Delete(args) => {
            let code = path_segment("--code", &args.code)?;
            Ok(ApiRequest::new("DELETE", format!("languages/{code}"), None))
        }
    }
}

/// Manage word classes in a language.
#[derive(Debug, Subcommand)]
pub(crate) enum WordClassCommand {
    /// List word classes in a language.
    List(LanguageArgs),
    /// Fetch a word class.
    #[command(visible_alias = "read")]
    Get(WordClassGetArgs),
    /// Create a word class.
    New(WordClassNewArgs),
    /// Update a word class.
    Edit(WordClassEditArgs),
    /// Delete a word class.
    Delete(WordClassDeleteArgs),
}

/// Arguments shared by commands scoped to a language.
#[derive(Debug, Args)]
pub(super) struct LanguageArgs {
    /// Language code.
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
}

#[derive(Debug, Args)]
pub(super) struct WordClassGetArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word-class abbreviation.
    #[arg(long)]
    abbreviation: String,
}

#[derive(Debug, Args)]
pub(super) struct WordClassNewArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word-class name.
    #[arg(long)]
    name: String,
    /// Word-class abbreviation.
    #[arg(long)]
    abbreviation: String,
    /// Optional editor notes.
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct WordClassEditArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Existing word-class abbreviation.
    #[arg(long)]
    abbreviation: String,
    /// Replacement name.
    #[arg(long)]
    name: Option<String>,
    /// Replacement abbreviation.
    #[arg(long = "new-abbreviation")]
    new_abbreviation: Option<String>,
    /// Replacement editor notes.
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct WordClassDeleteArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word-class abbreviation.
    #[arg(long)]
    abbreviation: String,
}

/// Convert a word-class command to its API request.
pub(crate) fn word_class_request(command: WordClassCommand) -> Result<ApiRequest> {
    match command {
        WordClassCommand::List(args) => {
            let language = language_segment(&args)?;
            Ok(ApiRequest::new(
                "GET",
                format!("languages/{language}/word-classes"),
                None,
            ))
        }
        WordClassCommand::Get(args) => {
            word_class_item_request("GET", args.language, args.abbreviation, None)
        }
        WordClassCommand::New(args) => {
            let language = language_segment(&args.language)?;
            let mut body = Map::from_iter([
                ("name".to_owned(), json!(args.name)),
                ("abbreviation".to_owned(), json!(args.abbreviation)),
            ]);
            insert_optional(&mut body, "notes", args.notes);
            Ok(ApiRequest::new(
                "POST",
                format!("languages/{language}/word-classes"),
                Some(body.into()),
            ))
        }
        WordClassCommand::Edit(args) => {
            let mut body = Map::new();
            insert_optional(&mut body, "name", args.name);
            insert_optional(&mut body, "abbreviation", args.new_abbreviation);
            insert_optional(&mut body, "notes", args.notes);
            word_class_item_request("PUT", args.language, args.abbreviation, Some(body.into()))
        }
        WordClassCommand::Delete(args) => {
            word_class_item_request("DELETE", args.language, args.abbreviation, None)
        }
    }
}

/// Manage word categories in a language.
#[derive(Debug, Subcommand)]
pub(crate) enum WordCategoryCommand {
    /// List word categories in a language.
    List(LanguageArgs),
    /// Fetch a word category.
    #[command(visible_alias = "read")]
    Get(WordCategoryGetArgs),
    /// Create a word category.
    New(WordCategoryNewArgs),
    /// Update a word category.
    Edit(WordCategoryEditArgs),
    /// Delete a word category.
    Delete(WordCategoryDeleteArgs),
}

#[derive(Debug, Args)]
pub(super) struct WordCategoryGetArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word-category abbreviation.
    #[arg(long)]
    abbreviation: String,
}

#[derive(Debug, Args)]
pub(super) struct WordCategoryNewArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word-category name.
    #[arg(long)]
    name: String,
    /// Word-category abbreviation.
    #[arg(long)]
    abbreviation: String,
    /// Optional editor notes.
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct WordCategoryEditArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Existing word-category abbreviation.
    #[arg(long)]
    abbreviation: String,
    /// Replacement name.
    #[arg(long)]
    name: Option<String>,
    /// Replacement abbreviation.
    #[arg(long = "new-abbreviation")]
    new_abbreviation: Option<String>,
    /// Replacement editor notes.
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct WordCategoryDeleteArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word-category abbreviation.
    #[arg(long)]
    abbreviation: String,
}

/// Convert a word-category command to its API request.
pub(crate) fn word_category_request(command: WordCategoryCommand) -> Result<ApiRequest> {
    match command {
        WordCategoryCommand::List(args) => {
            let language = language_segment(&args)?;
            Ok(ApiRequest::new(
                "GET",
                format!("languages/{language}/word-categories"),
                None,
            ))
        }
        WordCategoryCommand::Get(args) => {
            word_category_item_request("GET", args.language, args.abbreviation, None)
        }
        WordCategoryCommand::New(args) => {
            let language = language_segment(&args.language)?;
            let mut body = Map::from_iter([
                ("name".to_owned(), json!(args.name)),
                ("abbreviation".to_owned(), json!(args.abbreviation)),
            ]);
            insert_optional(&mut body, "notes", args.notes);
            Ok(ApiRequest::new(
                "POST",
                format!("languages/{language}/word-categories"),
                Some(body.into()),
            ))
        }
        WordCategoryCommand::Edit(args) => {
            let mut body = Map::new();
            insert_optional(&mut body, "name", args.name);
            insert_optional(&mut body, "abbreviation", args.new_abbreviation);
            insert_optional(&mut body, "notes", args.notes);
            word_category_item_request("PUT", args.language, args.abbreviation, Some(body.into()))
        }
        WordCategoryCommand::Delete(args) => {
            word_category_item_request("DELETE", args.language, args.abbreviation, None)
        }
    }
}

/// Manage definitions nested under a word.
#[derive(Debug, Subcommand)]
pub(crate) enum DefinitionCommand {
    /// List a word's definitions.
    List(DefinitionWordArgs),
    /// Fetch one definition.
    #[command(visible_alias = "read")]
    Get(DefinitionGetArgs),
    /// Create a definition.
    New(DefinitionNewArgs),
    /// Update a definition.
    Edit(DefinitionEditArgs),
    /// Delete a definition.
    Delete(DefinitionDeleteArgs),
}

#[derive(Debug, Args)]
pub(super) struct DefinitionWordArgs {
    #[command(flatten)]
    language: LanguageArgs,
    /// Word slug.
    #[arg(long)]
    slug: String,
    /// Word lemma number.
    #[arg(long)]
    lemma: i32,
}

#[derive(Debug, Args)]
pub(super) struct DefinitionGetArgs {
    #[command(flatten)]
    word: DefinitionWordArgs,
    /// Definition UUID.
    #[arg(long)]
    id: Uuid,
}

#[derive(Debug, Args)]
pub(super) struct DefinitionNewArgs {
    #[command(flatten)]
    word: DefinitionWordArgs,
    /// Definition text.
    #[arg(long = "def")]
    definition: String,
    /// Optional usage context.
    #[arg(long)]
    context: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct DefinitionEditArgs {
    #[command(flatten)]
    word: DefinitionWordArgs,
    /// Definition UUID.
    #[arg(long)]
    id: Uuid,
    /// Replacement definition text.
    #[arg(long = "def")]
    definition: Option<String>,
    /// Replacement usage context.
    #[arg(long)]
    context: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct DefinitionDeleteArgs {
    #[command(flatten)]
    word: DefinitionWordArgs,
    /// Definition UUID.
    #[arg(long)]
    id: Uuid,
}

/// Convert a definition command to its API request.
pub(crate) fn definition_request(command: DefinitionCommand) -> Result<ApiRequest> {
    match command {
        DefinitionCommand::List(args) => {
            let path = definition_collection_path(&args)?;
            Ok(ApiRequest::new("GET", path, None))
        }
        DefinitionCommand::Get(args) => definition_item_request("GET", args.word, args.id, None),
        DefinitionCommand::New(args) => {
            let path = definition_collection_path(&args.word)?;
            let mut body = Map::from_iter([("definition".to_owned(), json!(args.definition))]);
            insert_optional(&mut body, "context", args.context);
            Ok(ApiRequest::new("POST", path, Some(body.into())))
        }
        DefinitionCommand::Edit(args) => {
            let mut body = Map::new();
            insert_optional(&mut body, "definition", args.definition);
            insert_optional(&mut body, "context", args.context);
            definition_item_request("PUT", args.word, args.id, Some(body.into()))
        }
        DefinitionCommand::Delete(args) => {
            definition_item_request("DELETE", args.word, args.id, None)
        }
    }
}

fn insert_optional(body: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        body.insert(key.to_owned(), json!(value));
    }
}

fn language_segment(language: &LanguageArgs) -> Result<&str> {
    path_segment("--in", &language.language)
}

fn word_class_item_request(
    method: &'static str,
    language: LanguageArgs,
    abbreviation: String,
    body: Option<Value>,
) -> Result<ApiRequest> {
    let language = language_segment(&language)?;
    let abbreviation = path_segment("--abbreviation", &abbreviation)?;
    Ok(ApiRequest::new(
        method,
        format!("languages/{language}/word-classes/{abbreviation}"),
        body,
    ))
}

fn word_category_item_request(
    method: &'static str,
    language: LanguageArgs,
    abbreviation: String,
    body: Option<Value>,
) -> Result<ApiRequest> {
    let language = language_segment(&language)?;
    let abbreviation = path_segment("--abbreviation", &abbreviation)?;
    Ok(ApiRequest::new(
        method,
        format!("languages/{language}/word-categories/{abbreviation}"),
        body,
    ))
}

fn definition_collection_path(args: &DefinitionWordArgs) -> Result<String> {
    let language = language_segment(&args.language)?;
    let slug = path_segment("--slug", &args.slug)?;
    Ok(format!(
        "languages/{language}/words/{slug}/{}/definitions",
        args.lemma
    ))
}

fn definition_item_request(
    method: &'static str,
    word: DefinitionWordArgs,
    id: Uuid,
    body: Option<Value>,
) -> Result<ApiRequest> {
    let path = definition_collection_path(&word)?;
    let id = id.to_string();
    let id = path_segment("--id", &id)?;
    Ok(ApiRequest::new(method, format!("{path}/{id}"), body))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn language_edit_uses_only_supplied_update_fields() {
        let request = language_request(LanguageCommand::Edit(LanguageEditArgs {
            code: "pas".to_owned(),
            new_code: Some("paz".to_owned()),
            name: None,
            private: Some(true),
            description: None,
        }))
        .unwrap();

        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "languages/pas");
        assert_eq!(
            request.body,
            Some(json!({ "code": "paz", "private": true }))
        );
    }

    #[test]
    fn word_class_create_maps_to_nested_resource() {
        let request = word_class_request(WordClassCommand::New(WordClassNewArgs {
            language: LanguageArgs {
                language: "pas".to_owned(),
            },
            name: "verb".to_owned(),
            abbreviation: "v".to_owned(),
            notes: Some("transitive and intransitive".to_owned()),
        }))
        .unwrap();

        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "languages/pas/word-classes");
        assert_eq!(
            request.body,
            Some(json!({
                "name": "verb",
                "abbreviation": "v",
                "notes": "transitive and intransitive",
            }))
        );
    }

    #[test]
    fn definition_edit_maps_to_nested_item_and_payload() {
        let id = Uuid::parse_str("018f2bf9-6e35-7890-b8b5-95af61a1b3d5").unwrap();
        let request = definition_request(DefinitionCommand::Edit(DefinitionEditArgs {
            word: DefinitionWordArgs {
                language: LanguageArgs {
                    language: "pas".to_owned(),
                },
                slug: "kokebe".to_owned(),
                lemma: 2,
            },
            id,
            definition: Some("to smell unpleasant".to_owned()),
            context: None,
        }))
        .unwrap();

        assert_eq!(request.method, "PUT");
        assert_eq!(
            request.path,
            "languages/pas/words/kokebe/2/definitions/018f2bf9-6e35-7890-b8b5-95af61a1b3d5"
        );
        assert_eq!(
            request.body,
            Some(json!({ "definition": "to smell unpleasant" }))
        );
    }

    #[test]
    fn dynamic_path_segments_are_rejected() {
        let error = word_category_request(WordCategoryCommand::Delete(WordCategoryDeleteArgs {
            language: LanguageArgs {
                language: "pas/other".to_owned(),
            },
            abbreviation: "v".to_owned(),
        }))
        .unwrap_err();

        assert!(error.to_string().contains("--in"));
    }
}
