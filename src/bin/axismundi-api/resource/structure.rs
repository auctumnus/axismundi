//! Resource-oriented commands for the API's language structure resources.
#![allow(private_interfaces)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{ApiRequest, path_segment};

/// Commands for the API's language-structure resources.
#[derive(Debug, Subcommand)]
pub(crate) enum StructureCommand {
    /// Manage phonology tables.
    #[command(name = "phonology-table", visible_alias = "phonology")]
    PhonologyTable {
        #[command(subcommand)]
        command: PhonologyTableCommand,
    },
    /// Manage stored sound-change sets.
    #[command(name = "sound-change-set", visible_alias = "sound-change")]
    SoundChangeSet {
        #[command(subcommand)]
        command: SoundChangeSetCommand,
    },
    /// Manage language families.
    #[command(name = "language-family", visible_alias = "family")]
    LanguageFamily {
        #[command(subcommand)]
        command: LanguageFamilyCommand,
    },
    /// Manage language-family members.
    #[command(name = "language-family-member", visible_alias = "family-member")]
    LanguageFamilyMember {
        #[command(subcommand)]
        command: LanguageFamilyMemberCommand,
    },
}

pub(crate) fn structure_command_request(command: StructureCommand) -> Result<ApiRequest> {
    match command {
        StructureCommand::PhonologyTable { command } => phonology_table_request(command),
        StructureCommand::SoundChangeSet { command } => sound_change_set_request(command),
        StructureCommand::LanguageFamily { command } => language_family_request(command),
        StructureCommand::LanguageFamilyMember { command } => {
            language_family_member_request(command)
        }
    }
}

/// Manage a language's phonology tables.
#[derive(Debug, Subcommand)]
pub(super) enum PhonologyTableCommand {
    /// List phonology tables in a language.
    List(PhonologyTableListArgs),
    /// Create a phonology table.
    New(PhonologyTableNewArgs),
    /// Get a phonology table.
    #[command(visible_alias = "read")]
    Get(PhonologyTableIdArgs),
    /// Update a phonology table.
    #[command(visible_alias = "update")]
    Edit(PhonologyTableUpdateArgs),
    /// Delete a phonology table.
    Delete(PhonologyTableIdArgs),
    /// Swap the positions of two phonology tables.
    Swap(PhonologyTableSwapArgs),
}

#[derive(Debug, Args)]
pub(super) struct PhonologyTableListArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    q: Option<String>,
    #[arg(long)]
    created_before: Option<String>,
    #[arg(long)]
    created_after: Option<String>,
    #[command(flatten)]
    page: PageArgs,
}

#[derive(Debug, Args)]
pub(super) struct PhonologyTableNewArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: Option<String>,
    #[command(flatten)]
    body: JsonInputArgs,
}

#[derive(Debug, Args)]
pub(super) struct PhonologyTableIdArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    id: Uuid,
}

#[derive(Debug, Args)]
pub(super) struct PhonologyTableUpdateArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    id: Uuid,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[command(flatten)]
    body: OptionalJsonInputArgs,
}

#[derive(Debug, Args)]
pub(super) struct PhonologyTableSwapArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    id1: Uuid,
    #[arg(long)]
    id2: Uuid,
}

pub(super) fn phonology_table_request(command: PhonologyTableCommand) -> Result<ApiRequest> {
    match command {
        PhonologyTableCommand::List(args) => {
            let language = path_segment("--in", &args.language)?;
            let path = query_path(
                format!("languages/{language}/phonology-tables"),
                [
                    ("q", args.q),
                    ("created_before", args.created_before),
                    ("created_after", args.created_after),
                    ("offset", args.page.offset.map(|value| value.to_string())),
                    ("limit", args.page.limit.map(|value| value.to_string())),
                ],
            );
            Ok(ApiRequest::new("GET", path, None))
        }
        PhonologyTableCommand::New(args) => {
            let language = path_segment("--in", &args.language)?;
            let body = required_json(&args.body, "--body or --body-file")?;
            Ok(ApiRequest::new(
                "POST",
                format!("languages/{language}/phonology-tables"),
                Some(json!({ "name": args.name, "description": args.description, "body": body })),
            ))
        }
        PhonologyTableCommand::Get(args) => table_by_id_request("GET", args, None),
        PhonologyTableCommand::Delete(args) => table_by_id_request("DELETE", args, None),
        PhonologyTableCommand::Edit(args) => {
            let language = path_segment("--in", &args.language)?;
            let mut body = serde_json::Map::new();
            put_optional(&mut body, "name", args.name);
            put_optional(&mut body, "description", args.description);
            if let Some(value) = optional_json(&args.body)? {
                body.insert("body".into(), value);
            }
            ensure_nonempty_update(&body)?;
            Ok(ApiRequest::new(
                "PUT",
                format!("languages/{language}/phonology-tables/{}", args.id),
                Some(Value::Object(body)),
            ))
        }
        PhonologyTableCommand::Swap(args) => {
            let language = path_segment("--in", &args.language)?;
            Ok(ApiRequest::new(
                "POST",
                format!("languages/{language}/phonology-tables/swap"),
                Some(json!({ "id1": args.id1, "id2": args.id2 })),
            ))
        }
    }
}

fn table_by_id_request(
    method: &'static str,
    args: PhonologyTableIdArgs,
    body: Option<Value>,
) -> Result<ApiRequest> {
    let language = path_segment("--in", &args.language)?;
    Ok(ApiRequest::new(
        method,
        format!("languages/{language}/phonology-tables/{}", args.id),
        body,
    ))
}

/// Manage a language's stored sound-change sets.
#[derive(Debug, Subcommand)]
pub(super) enum SoundChangeSetCommand {
    List(SoundChangeSetListArgs),
    New(SoundChangeSetNewArgs),
    #[command(visible_alias = "read")]
    Get(SoundChangeSetIdArgs),
    #[command(visible_alias = "update")]
    Edit(SoundChangeSetUpdateArgs),
    Delete(SoundChangeSetIdArgs),
    /// Run a stored sound-change set against one or more input words.
    Run(SoundChangeSetRunArgs),
}

#[derive(Debug, Args)]
pub(super) struct SoundChangeSetListArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    q: Option<String>,
    #[arg(long)]
    author: Option<String>,
    #[command(flatten)]
    page: PageArgs,
}

#[derive(Debug, Args)]
pub(super) struct SoundChangeSetNewArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    changes: String,
}

#[derive(Debug, Args)]
pub(super) struct SoundChangeSetIdArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    id: Uuid,
}

#[derive(Debug, Args)]
pub(super) struct SoundChangeSetUpdateArgs {
    #[arg(long = "in", value_name = "LANGUAGE", env = "AXM_DEFAULT_LANGUAGE")]
    language: String,
    #[arg(long)]
    id: Uuid,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    changes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct SoundChangeSetRunArgs {
    #[arg(long)]
    id: Uuid,
    /// Input word. Repeat this flag to run multiple words.
    #[arg(long = "word", required = true)]
    input_words: Vec<String>,
}

pub(super) fn sound_change_set_request(command: SoundChangeSetCommand) -> Result<ApiRequest> {
    match command {
        SoundChangeSetCommand::List(args) => {
            let language = path_segment("--in", &args.language)?;
            Ok(ApiRequest::new(
                "GET",
                query_path(
                    format!("languages/{language}/sound-change-sets"),
                    [
                        ("q", args.q),
                        ("author", args.author),
                        ("offset", args.page.offset.map(|value| value.to_string())),
                        ("limit", args.page.limit.map(|value| value.to_string())),
                    ],
                ),
                None,
            ))
        }
        SoundChangeSetCommand::New(args) => {
            let language = path_segment("--in", &args.language)?;
            Ok(ApiRequest::new(
                "POST",
                format!("languages/{language}/sound-change-sets"),
                Some(json!({
                    "name": args.name, "description": args.description, "changes": args.changes,
                })),
            ))
        }
        SoundChangeSetCommand::Get(args) => sound_change_set_by_id_request("GET", args),
        SoundChangeSetCommand::Delete(args) => sound_change_set_by_id_request("DELETE", args),
        SoundChangeSetCommand::Edit(args) => {
            let language = path_segment("--in", &args.language)?;
            let mut body = serde_json::Map::new();
            put_optional(&mut body, "name", args.name);
            put_optional(&mut body, "description", args.description);
            put_optional(&mut body, "changes", args.changes);
            ensure_nonempty_update(&body)?;
            Ok(ApiRequest::new(
                "PUT",
                format!("languages/{language}/sound-change-sets/{}", args.id),
                Some(Value::Object(body)),
            ))
        }
        SoundChangeSetCommand::Run(args) => Ok(ApiRequest::new(
            "POST",
            format!("sound-change-sets/{}/run", args.id),
            Some(json!({ "input_words": args.input_words })),
        )),
    }
}

fn sound_change_set_by_id_request(
    method: &'static str,
    args: SoundChangeSetIdArgs,
) -> Result<ApiRequest> {
    let language = path_segment("--in", &args.language)?;
    Ok(ApiRequest::new(
        method,
        format!("languages/{language}/sound-change-sets/{}", args.id),
        None,
    ))
}

/// Manage language families. The server currently has no update or delete routes for families.
#[derive(Debug, Subcommand)]
pub(super) enum LanguageFamilyCommand {
    List(LanguageFamilyListArgs),
    New(LanguageFamilyNewArgs),
    #[command(visible_alias = "read")]
    Get(LanguageFamilyGetArgs),
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyListArgs {
    #[arg(long)]
    q: Option<String>,
    #[arg(long)]
    owner: Option<String>,
    #[arg(long)]
    has_language: Option<String>,
    #[command(flatten)]
    page: PageArgs,
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyNewArgs {
    #[arg(long)]
    code: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyGetArgs {
    #[arg(long)]
    code: String,
}

pub(super) fn language_family_request(command: LanguageFamilyCommand) -> Result<ApiRequest> {
    match command {
        LanguageFamilyCommand::List(args) => Ok(ApiRequest::new(
            "GET",
            query_path(
                "language-families".to_owned(),
                [
                    ("q", args.q),
                    ("owner", args.owner),
                    ("has_language", args.has_language),
                    ("offset", args.page.offset.map(|value| value.to_string())),
                    ("limit", args.page.limit.map(|value| value.to_string())),
                ],
            ),
            None,
        )),
        LanguageFamilyCommand::New(args) => {
            path_segment("--code", &args.code)?;
            Ok(ApiRequest::new(
                "POST",
                "language-families".into(),
                Some(json!({
                    "code": args.code, "name": args.name, "description": args.description,
                })),
            ))
        }
        LanguageFamilyCommand::Get(args) => {
            let code = path_segment("--code", &args.code)?;
            Ok(ApiRequest::new(
                "GET",
                format!("language-families/{code}"),
                None,
            ))
        }
    }
}

/// Manage language-family members. Member routes intentionally use `/language-family` (singular).
#[derive(Debug, Subcommand)]
pub(super) enum LanguageFamilyMemberCommand {
    List(LanguageFamilyMemberListArgs),
    New(LanguageFamilyMemberNewArgs),
    /// Get the root member of a family.
    Root(LanguageFamilyMemberRootArgs),
    #[command(visible_alias = "read")]
    Get(LanguageFamilyMemberReferenceArgs),
    Delete(LanguageFamilyMemberReferenceArgs),
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyMemberListArgs {
    /// Restrict the search to this family. It also selects the family-member route.
    #[arg(long)]
    family: Option<String>,
    /// List only direct children of this member. Requires --family.
    #[arg(long, requires = "family", conflicts_with = "parent_language")]
    parent_id: Option<Uuid>,
    /// List only direct children of this language member. Requires --family.
    #[arg(long, requires = "family", conflicts_with = "parent_id")]
    parent_language: Option<String>,
    #[arg(long)]
    language: Option<String>,
    #[arg(long, value_enum)]
    relation_type: Option<FamilyRelationType>,
    #[arg(long)]
    q: Option<String>,
    #[command(flatten)]
    page: PageArgs,
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyMemberNewArgs {
    #[arg(long)]
    family: String,
    /// Add the member below this member ID.
    #[arg(long, conflicts_with = "parent_language")]
    parent_id: Option<Uuid>,
    /// Add the member below the existing member for this language code.
    #[arg(long, conflicts_with = "parent_id")]
    parent_language: Option<String>,
    #[arg(long, value_enum)]
    relation_type: FamilyRelationType,
    #[arg(long)]
    language: Option<String>,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyMemberReferenceArgs {
    #[arg(long)]
    family: String,
    #[arg(
        long,
        required_unless_present = "language",
        conflicts_with = "language"
    )]
    id: Option<Uuid>,
    #[arg(long, required_unless_present = "id", conflicts_with = "id")]
    language: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct LanguageFamilyMemberRootArgs {
    #[arg(long)]
    family: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum FamilyRelationType {
    Descendant,
    Hybrid,
}

impl FamilyRelationType {
    fn api_value(self) -> &'static str {
        match self {
            Self::Descendant => "descendant",
            Self::Hybrid => "hybrid",
        }
    }
}

pub(super) fn language_family_member_request(
    command: LanguageFamilyMemberCommand,
) -> Result<ApiRequest> {
    match command {
        LanguageFamilyMemberCommand::List(args) => member_list_request(args),
        LanguageFamilyMemberCommand::New(args) => {
            let family = path_segment("--family", &args.family)?;
            let path = if let Some(parent_id) = args.parent_id {
                format!("language-family/{family}/members/by-id/{parent_id}/children")
            } else if let Some(parent_language) = args.parent_language {
                let parent_language = path_segment("--parent-language", &parent_language)?;
                format!("language-family/{family}/members/by-code/{parent_language}/children")
            } else {
                format!("language-family/{family}/members")
            };
            if let Some(language) = args.language.as_deref() {
                path_segment("--language", language)?;
            }
            Ok(ApiRequest::new(
                "POST",
                path,
                Some(json!({
                    "language_code": args.language,
                    "title": args.title,
                    "relation_type": args.relation_type.api_value(),
                    "notes": args.notes,
                })),
            ))
        }
        LanguageFamilyMemberCommand::Root(args) => {
            let family = path_segment("--family", &args.family)?;
            Ok(ApiRequest::new(
                "GET",
                format!("language-family/{family}/root"),
                None,
            ))
        }
        LanguageFamilyMemberCommand::Get(args) => member_reference_request("GET", args),
        LanguageFamilyMemberCommand::Delete(args) => member_reference_request("DELETE", args),
    }
}

fn member_list_request(args: LanguageFamilyMemberListArgs) -> Result<ApiRequest> {
    if let Some(language) = args.language.as_deref() {
        path_segment("--language", language)?;
    }
    let path = match (
        args.family.as_deref(),
        args.parent_id,
        args.parent_language.as_deref(),
    ) {
        (Some(family), Some(parent_id), None) => {
            let family = path_segment("--family", family)?;
            format!("language-family/{family}/members/by-id/{parent_id}/children")
        }
        (Some(family), None, Some(parent_language)) => {
            let family = path_segment("--family", family)?;
            let parent_language = path_segment("--parent-language", parent_language)?;
            format!("language-family/{family}/members/by-code/{parent_language}/children")
        }
        (Some(family), None, None) => {
            let family = path_segment("--family", family)?;
            format!("language-family/{family}/members")
        }
        (None, None, None) => "language-family-members".into(),
        _ => bail!("--parent-id and --parent-language require --family and cannot be combined"),
    };
    Ok(ApiRequest::new(
        "GET",
        query_path(
            path,
            [
                ("language_code", args.language),
                (
                    "relation_type",
                    args.relation_type.map(|value| value.api_value().to_owned()),
                ),
                ("q", args.q),
                ("offset", args.page.offset.map(|value| value.to_string())),
                ("limit", args.page.limit.map(|value| value.to_string())),
            ],
        ),
        None,
    ))
}

fn member_reference_request(
    method: &'static str,
    args: LanguageFamilyMemberReferenceArgs,
) -> Result<ApiRequest> {
    let family = path_segment("--family", &args.family)?;
    let path = match (args.id, args.language.as_deref()) {
        (Some(id), None) => format!("language-family/{family}/members/by-id/{id}"),
        (None, Some(language)) => {
            let language = path_segment("--language", language)?;
            format!("language-family/{family}/members/by-code/{language}")
        }
        _ => bail!("provide exactly one of --id or --language"),
    };
    Ok(ApiRequest::new(method, path, None))
}

#[derive(Debug, Args)]
struct PageArgs {
    #[arg(long)]
    offset: Option<i64>,
    #[arg(long)]
    limit: Option<i64>,
}

#[derive(Debug, Args)]
struct JsonInputArgs {
    /// JSON object supplied inline.
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    /// Read the JSON object from a file.
    #[arg(long, value_name = "FILE", conflicts_with = "body")]
    body_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct OptionalJsonInputArgs {
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    #[arg(long, value_name = "FILE", conflicts_with = "body")]
    body_file: Option<PathBuf>,
}

fn required_json(args: &JsonInputArgs, flags: &str) -> Result<Value> {
    optional_json_inner(args.body.as_deref(), args.body_file.as_deref())?
        .with_context(|| format!("one of {flags} is required"))
}

fn optional_json(args: &OptionalJsonInputArgs) -> Result<Option<Value>> {
    optional_json_inner(args.body.as_deref(), args.body_file.as_deref())
}

fn optional_json_inner(inline: Option<&str>, file: Option<&Path>) -> Result<Option<Value>> {
    let source = match (inline, file) {
        (Some(value), None) => value.to_owned(),
        (None, Some(path)) => fs::read_to_string(path)
            .with_context(|| format!("failed to read JSON from {}", path.display()))?,
        (None, None) => return Ok(None),
        (Some(_), Some(_)) => bail!("use either --body or --body-file, not both"),
    };
    let value: Value = serde_json::from_str(&source).context("--body must contain valid JSON")?;
    if !value.is_object() {
        bail!("--body JSON must be an object");
    }
    Ok(Some(value))
}

fn put_optional(map: &mut serde_json::Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.into(), Value::String(value));
    }
}

fn ensure_nonempty_update(body: &serde_json::Map<String, Value>) -> Result<()> {
    if body.is_empty() {
        bail!("provide at least one field to update");
    }
    Ok(())
}

fn query_path<const N: usize>(path: String, pairs: [(&'static str, Option<String>); N]) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        if let Some(value) = value {
            query.append_pair(key, &value);
        }
    }
    let query = query.finish();
    if query.is_empty() {
        path
    } else {
        format!("{path}?{query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "00000000-0000-0000-0000-000000000001";

    #[test]
    fn phonology_table_create_encodes_validated_body() {
        let request = phonology_table_request(PhonologyTableCommand::New(PhonologyTableNewArgs {
            language: "pas".into(),
            name: "Consonants".into(),
            description: None,
            body: JsonInputArgs {
                body: Some(r#"{"rows":[],"columns":[],"annotations":[]}"#.into()),
                body_file: None,
            },
        }))
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "languages/pas/phonology-tables");
        assert_eq!(request.body.unwrap()["body"]["rows"], json!([]));
    }

    #[test]
    fn stored_sound_change_run_uses_input_words_payload() {
        let request = sound_change_set_request(SoundChangeSetCommand::Run(SoundChangeSetRunArgs {
            id: FIRST.parse().unwrap(),
            input_words: vec!["pater".into(), "mater".into()],
        }))
        .unwrap();
        assert_eq!(request.path, format!("sound-change-sets/{FIRST}/run"));
        assert_eq!(
            request.body.unwrap(),
            json!({"input_words":["pater", "mater"]})
        );
    }

    #[test]
    fn family_member_child_uses_singular_family_route() {
        let request = language_family_member_request(LanguageFamilyMemberCommand::New(
            LanguageFamilyMemberNewArgs {
                family: "indo-european".into(),
                parent_id: Some(FIRST.parse().unwrap()),
                parent_language: None,
                relation_type: FamilyRelationType::Descendant,
                language: Some("pas".into()),
                title: None,
                notes: Some("test".into()),
            },
        ))
        .unwrap();
        assert_eq!(
            request.path,
            format!("language-family/indo-european/members/by-id/{FIRST}/children")
        );
        assert_eq!(
            request.body.unwrap(),
            json!({"language_code":"pas", "title":null, "relation_type":"descendant", "notes":"test"})
        );
    }

    #[test]
    fn member_by_language_escapes_query_and_selects_by_code_route() {
        let request = language_family_member_request(LanguageFamilyMemberCommand::List(
            LanguageFamilyMemberListArgs {
                family: Some("family".into()),
                parent_id: None,
                parent_language: Some("parent".into()),
                language: Some("child".into()),
                relation_type: Some(FamilyRelationType::Hybrid),
                q: Some("a & b".into()),
                page: PageArgs {
                    offset: Some(0),
                    limit: Some(25),
                },
            },
        ))
        .unwrap();
        assert_eq!(
            request.path,
            "language-family/family/members/by-code/parent/children?language_code=child&relation_type=hybrid&q=a+%26+b&offset=0&limit=25"
        );
    }
}
