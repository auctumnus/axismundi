//! Command-line client for the Axis Mundi HTTP API.

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, IsTerminal as _, Read as _, Write as _},
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::{
    Client, Method, Url,
    header::{
        AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue,
        RETRY_AFTER, TRANSFER_ENCODING,
    },
    multipart::{Form, Part},
    redirect::Policy,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[path = "axismundi-api/resource/mod.rs"]
mod resource;

const DEFAULT_BASE_URL: &str = "https://axismundi.app/api";
const DEFAULT_LANGUAGE_ENV: &str = "AXM_DEFAULT_LANGUAGE";

#[derive(Debug, Parser)]
#[command(
    name = "axm",
    version,
    about = "A safe, scriptable client for the Axis Mundi API"
)]
struct Cli {
    /// API base URL. The default targets the public Axismundi instance.
    #[arg(long, global = true, env = "AXISMUNDI_API_URL")]
    base_url: Option<String>,

    /// Website base URL for terminal hyperlinks. Defaults to the API URL without /api.
    #[arg(long, global = true, env = "AXISMUNDI_WEB_URL")]
    web_url: Option<String>,

    /// API token. Prefer AXISMUNDI_API_TOKEN or --token-file to avoid shell history.
    #[arg(
        long,
        global = true,
        env = "AXISMUNDI_API_TOKEN",
        conflicts_with = "token_file"
    )]
    token: Option<String>,

    /// File containing an API token.
    #[arg(
        long,
        global = true,
        env = "AXISMUNDI_API_TOKEN_FILE",
        value_name = "FILE",
        conflicts_with = "token"
    )]
    token_file: Option<PathBuf>,

    /// Request timeout in seconds. Set to 0 to disable the timeout.
    #[arg(
        long,
        global = true,
        env = "AXISMUNDI_API_TIMEOUT",
        default_value_t = 60
    )]
    timeout: u64,

    /// Follow redirects. Redirects are disabled by default so status and Location stay visible.
    #[arg(long, global = true)]
    follow: bool,

    /// Permit sending a token over non-loopback HTTP. HTTPS is always allowed.
    #[arg(long, global = true)]
    allow_insecure_http: bool,

    /// How to write response bodies to stdout.
    #[arg(long, global = true, value_enum, default_value_t = OutputMode::Auto)]
    output: OutputMode,

    /// Write the response body unchanged. This is the default when stdout is not interactive.
    #[arg(long = "json", global = true, conflicts_with = "output")]
    raw_json: bool,

    /// Number of automatic retries for 429 responses to safe read requests.
    #[arg(long, global = true, default_value_t = 3)]
    max_retries: u8,

    /// Also retry writes after 429 responses. This can repeat a write if a proxy responds late.
    #[arg(long, global = true)]
    retry_writes: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage dictionary words.
    Word {
        #[command(subcommand)]
        command: WordCommand,
    },

    /// Manage languages.
    Language {
        #[command(subcommand)]
        command: resource::dictionary::LanguageCommand,
    },

    /// Manage the word classes in a language.
    WordClass {
        #[command(subcommand)]
        command: resource::dictionary::WordClassCommand,
    },

    /// Manage the word categories in a language.
    WordCategory {
        #[command(subcommand)]
        command: resource::dictionary::WordCategoryCommand,
    },

    /// Manage definitions belonging to a word.
    Definition {
        #[command(subcommand)]
        command: resource::dictionary::DefinitionCommand,
    },

    /// Manage translatable texts, translations, quotations, and news.
    Content {
        #[command(subcommand)]
        command: resource::content::ContentCommand,
    },

    /// Manage phonology tables, sound-change sets, and language families.
    Structure {
        #[command(subcommand)]
        command: resource::structure::StructureCommand,
    },

    /// Send an API request. PATH may start with / but is always resolved under --base-url.
    #[command(hide = true)]
    Request(RequestArgs),
}

#[derive(Debug, Subcommand)]
enum WordCommand {
    /// List words in a language.
    List(WordListArgs),
    /// Fetch one word by slug and lemma number.
    #[command(visible_alias = "read")]
    Get(WordLocatorArgs),
    /// Create a word in a language.
    New(WordNewArgs),
    /// Update a word by slug and lemma number.
    Edit(WordEditArgs),
    /// Delete a word by slug and lemma number.
    Delete(WordLocatorArgs),
}

#[derive(Debug, Args)]
struct WordListArgs {
    /// Language code.
    #[arg(long = "in", value_name = "LANGUAGE", env = DEFAULT_LANGUAGE_ENV)]
    language: String,

    /// Search word forms and definitions.
    #[arg(long)]
    q: Option<String>,

    /// Restrict results to a normalized word slug.
    #[arg(long = "exact-slug")]
    exact_slug: Option<String>,

    /// Restrict results to a word-class abbreviation.
    #[arg(long = "class", value_name = "ABBREVIATION")]
    word_class: Option<String>,

    /// Return words created before this RFC 3339 timestamp.
    #[arg(long)]
    created_before: Option<String>,

    /// Return words created after this RFC 3339 timestamp.
    #[arg(long)]
    created_after: Option<String>,

    /// Restrict results to a category. Repeat for several categories.
    #[arg(long = "category", value_name = "ABBREVIATION")]
    categories: Vec<String>,

    /// Number of results to skip.
    #[arg(long)]
    offset: Option<i64>,

    /// Maximum results to return.
    #[arg(long)]
    limit: Option<i64>,
}

#[derive(Debug, Args)]
struct WordLocatorArgs {
    /// Language code.
    #[arg(long = "in", value_name = "LANGUAGE", env = DEFAULT_LANGUAGE_ENV)]
    language: String,

    /// Normalized word slug.
    #[arg(long)]
    slug: String,

    /// Lemma number that distinguishes homographs with the same slug.
    #[arg(long)]
    lemma: i32,
}

#[derive(Debug, Args)]
struct WordNewArgs {
    /// Language code for the word.
    #[arg(long = "in", value_name = "LANGUAGE", env = DEFAULT_LANGUAGE_ENV)]
    language: String,

    /// Surface form of the word.
    #[arg(long)]
    word: String,

    /// Definition. Repeat --def to add several ordered definitions.
    #[arg(long = "def", required = true, value_name = "DEFINITION")]
    definitions: Vec<String>,

    /// Word-class abbreviation, such as n or v.
    #[arg(long = "class", value_name = "ABBREVIATION")]
    word_class: String,

    /// IPA transcription.
    #[arg(long)]
    ipa: Option<String>,

    /// Editor notes for the word.
    #[arg(long)]
    notes: Option<String>,

    /// Word-category abbreviation. Repeat --category to add several categories.
    #[arg(long = "category", value_name = "ABBREVIATION")]
    categories: Vec<String>,
}

#[derive(Debug, Args)]
struct WordEditArgs {
    #[command(flatten)]
    locator: WordLocatorArgs,

    /// Replacement surface form.
    #[arg(long)]
    word: Option<String>,

    /// Replacement word-class abbreviation.
    #[arg(long = "class", value_name = "ABBREVIATION")]
    word_class: Option<String>,

    /// Replacement IPA transcription.
    #[arg(long)]
    ipa: Option<String>,

    /// Remove the IPA transcription.
    #[arg(long, conflicts_with = "ipa")]
    clear_ipa: bool,

    /// Replacement editor notes.
    #[arg(long)]
    notes: Option<String>,

    /// Remove the editor notes.
    #[arg(long, conflicts_with = "notes")]
    clear_notes: bool,

    /// Replacement category abbreviation. Repeat to set several categories.
    #[arg(long = "category", value_name = "ABBREVIATION")]
    categories: Option<Vec<String>>,

    /// Remove all word categories.
    #[arg(long, conflicts_with = "categories")]
    clear_categories: bool,

    /// Replacement JSON value for the word's extra data.
    #[arg(long, value_name = "JSON")]
    extra: Option<String>,

    /// Remove the word's extra data.
    #[arg(long, conflicts_with = "extra")]
    clear_extra: bool,
}

#[derive(Debug, Args)]
struct RequestArgs {
    /// HTTP method, such as GET, POST, PUT, PATCH, or DELETE.
    method: String,

    /// API path and optional query string, for example languages/example/words.
    path: String,

    /// Add a query parameter. Repeat the option for repeated keys.
    #[arg(short = 'q', long, value_name = "KEY=VALUE", value_parser = parse_key_value)]
    query: Vec<KeyValue>,

    /// Add an HTTP header.
    #[arg(short = 'H', long, value_name = "NAME: VALUE", value_parser = parse_header)]
    header: Vec<Header>,

    /// JSON request body. Use --json to request raw response output.
    #[arg(long = "body", value_name = "JSON", conflicts_with_all = ["json_file", "image"])]
    json: Option<String>,

    /// JSON request body read from FILE. Use - to read from standard input.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["json", "image"])]
    json_file: Option<PathBuf>,

    /// Upload IMAGE as multipart/form-data with the API's image field name.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["json", "json_file"])]
    image: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputMode {
    /// Pretty-print JSON responses for interactive stdout; otherwise preserve response bytes.
    Auto,
    /// Pretty-print JSON responses; write non-JSON responses unchanged with a warning.
    Pretty,
    /// Always preserve the response bytes.
    Raw,
}

fn output_mode(mode: OutputMode, json: bool, stdout_is_terminal: bool) -> OutputMode {
    if json {
        OutputMode::Raw
    } else if mode == OutputMode::Auto && stdout_is_terminal {
        OutputMode::Pretty
    } else if mode == OutputMode::Auto {
        OutputMode::Raw
    } else {
        mode
    }
}

#[derive(Debug, Clone)]
struct KeyValue {
    key: String,
    value: String,
}

#[derive(Debug, Clone)]
struct Header {
    name: HeaderName,
    value: HeaderValue,
}

#[derive(Debug)]
struct ClientConfig {
    base_url: Url,
    web_url: Option<Url>,
    token: Option<String>,
    timeout: Option<Duration>,
    follow_redirects: bool,
    allow_insecure_http: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AxmConfig {
    /// Base URL for API requests, including `/api` when appropriate.
    api_url: Option<String>,
    /// Base URL for website links shown in interactive word listings.
    web_url: Option<String>,
    /// File containing the API token. Relative paths are resolved from this config file.
    token_file: Option<PathBuf>,
    /// Language code supplied to `--in` when no command-line value is given.
    default_language: Option<String>,
}

#[derive(Debug)]
struct ResponseBody {
    status: reqwest::StatusCode,
    content_type: Option<String>,
    bytes: Vec<u8>,
}

fn main() -> ExitCode {
    let config = match load_axm_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error:#}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = set_configured_default_language(&config) {
        eprintln!("error: {error:#}");
        return ExitCode::FAILURE;
    }
    run_main(Cli::parse(), config)
}

#[tokio::main]
async fn run_main(cli: Cli, config: AxmConfig) -> ExitCode {
    match run(cli, config).await {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli, file_config: AxmConfig) -> Result<u8> {
    if cli.max_retries > 10 {
        bail!("--max-retries must be at most 10");
    }
    let token_file = if cli.token.is_some() {
        None
    } else {
        cli.token_file
            .as_deref()
            .or(file_config.token_file.as_deref())
    };
    let base_url = parse_base_url(
        cli.base_url
            .as_deref()
            .or(file_config.api_url.as_deref())
            .unwrap_or(DEFAULT_BASE_URL),
    )?;
    let web_url = cli
        .web_url
        .as_deref()
        .or(file_config.web_url.as_deref())
        .map(parse_web_url)
        .transpose()?
        .or_else(|| derive_web_url(&base_url));
    let config = ClientConfig {
        base_url,
        web_url,
        token: load_token(cli.token, token_file)?,
        timeout: (cli.timeout != 0).then(|| Duration::from_secs(cli.timeout)),
        follow_redirects: cli.follow,
        allow_insecure_http: cli.allow_insecure_http,
    };
    ensure_token_transport_is_safe(&config)?;

    let client = build_client(&config)?;
    let args = match cli.command {
        Command::Word { command } => word_command_request(command)?,
        Command::Language { command } => {
            api_request_args(resource::dictionary::language_request(command)?)?
        }
        Command::WordClass { command } => {
            api_request_args(resource::dictionary::word_class_request(command)?)?
        }
        Command::WordCategory { command } => {
            api_request_args(resource::dictionary::word_category_request(command)?)?
        }
        Command::Definition { command } => {
            api_request_args(resource::dictionary::definition_request(command)?)?
        }
        Command::Content { command } => {
            api_request_args(resource::content::content_command_request(command)?)?
        }
        Command::Structure { command } => {
            api_request_args(resource::structure::structure_command_request(command)?)?
        }
        Command::Request(args) => args,
    };
    let response =
        send_with_rate_limit_retries(&client, &config, &args, cli.max_retries, cli.retry_writes)
            .await?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response
        .bytes()
        .await
        .context("failed to read response body")?;
    let mut response = ResponseBody {
        status,
        content_type,
        bytes: bytes.to_vec(),
    };

    let stdout_is_terminal = io::stdout().is_terminal();
    let output = output_mode(cli.output, cli.raw_json, stdout_is_terminal);
    if response.status.is_success()
        && output == OutputMode::Pretty
        && stdout_is_terminal
        && let Some(language) = word_list_language(&args)
    {
        enrich_word_list_with_definitions(
            &client,
            &config,
            &mut response,
            &language,
            cli.max_retries,
        )
        .await?;
    }
    write_response(
        &response,
        output,
        &args,
        stdout_is_terminal,
        config.web_url.as_ref(),
    )?;

    if response.status.is_success() {
        Ok(0)
    } else {
        eprintln!("HTTP {}", response.status);
        Ok(1)
    }
}

async fn enrich_word_list_with_definitions(
    client: &Client,
    config: &ClientConfig,
    response: &mut ResponseBody,
    language: &str,
    max_retries: u8,
) -> Result<()> {
    if !is_json_content_type(response.content_type.as_deref()) {
        return Ok(());
    }
    let mut body: Value = match serde_json::from_slice(&response.bytes) {
        Ok(body) => body,
        Err(_) => return Ok(()),
    };
    let Some(items) = body.get("items").and_then(Value::as_array) else {
        return Ok(());
    };
    let missing_previews: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.get("preview_definitions").is_none())
        .filter_map(|(index, item)| {
            Some((
                index,
                item.get("slug")?.as_str()?.to_owned(),
                item.get("lemma")?.as_i64()?,
            ))
        })
        .collect();

    for (index, slug, lemma) in missing_previews {
        match fetch_definition_preview(client, config, language, &slug, lemma, max_retries).await {
            Ok(preview_definitions) => {
                body["items"][index]["preview_definitions"] = json!(preview_definitions)
            }
            Err(error) => {
                eprintln!("warning: could not load definitions for {slug}/{lemma}: {error:#}")
            }
        }
    }
    response.bytes = serde_json::to_vec(&body).context("failed to encode enriched word list")?;
    Ok(())
}

async fn fetch_definition_preview(
    client: &Client,
    config: &ClientConfig,
    language: &str,
    slug: &str,
    lemma: i64,
    max_retries: u8,
) -> Result<Vec<String>> {
    let path = format!("languages/{language}/words/{slug}/{lemma}/definitions");
    let request = RequestArgs {
        method: "GET".to_owned(),
        path,
        query: vec![KeyValue {
            key: "limit".to_owned(),
            value: "5".to_owned(),
        }],
        header: Vec::new(),
        json: None,
        json_file: None,
        image: None,
    };
    let response =
        send_with_rate_limit_retries(client, config, &request, max_retries, false).await?;
    if !response.status().is_success() {
        bail!("HTTP {}", response.status());
    }
    let body: Value = response
        .json()
        .await
        .context("invalid definitions response")?;
    Ok(body
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("definition").and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

async fn send_with_rate_limit_retries(
    client: &Client,
    config: &ClientConfig,
    args: &RequestArgs,
    max_retries: u8,
    retry_writes: bool,
) -> Result<reqwest::Response> {
    let method = Method::from_bytes(args.method.as_bytes())
        .with_context(|| format!("invalid HTTP method {:?}", args.method))?;
    let retries_allowed = is_safe_read_method(&method) || retry_writes;

    for retry in 0..=max_retries {
        let request = build_request(client, config, args).await?;
        let response = client.execute(request).await.context("request failed")?;
        if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS
            || !retries_allowed
            || retry == max_retries
        {
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && !retries_allowed {
                eprintln!(
                    "rate limited; writes are not retried automatically. Re-run with --retry-writes to opt in."
                );
            }
            return Ok(response);
        }

        let delay = rate_limit_delay(response.headers(), retry);
        eprintln!(
            "rate limited; retrying in {} second{} ({}/{})",
            delay.as_secs(),
            if delay.as_secs() == 1 { "" } else { "s" },
            retry + 1,
            max_retries
        );
        tokio::time::sleep(delay).await;
    }
    unreachable!("retry loop always returns")
}

fn is_safe_read_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn rate_limit_delay(headers: &HeaderMap, retry: u8) -> Duration {
    if let Some(seconds) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_secs(seconds);
    }
    Duration::from_secs(2_u64.saturating_pow(u32::from(retry)).min(30))
}

fn word_command_request(command: WordCommand) -> Result<RequestArgs> {
    match command {
        WordCommand::List(args) => word_list_request(args),
        WordCommand::Get(args) => word_item_request("GET", args, None),
        WordCommand::New(args) => word_new_request(args),
        WordCommand::Edit(args) => word_edit_request(args),
        WordCommand::Delete(args) => word_item_request("DELETE", args, None),
    }
}

fn api_request_args(request: resource::ApiRequest) -> Result<RequestArgs> {
    Ok(RequestArgs {
        method: request.method.to_owned(),
        path: request.path,
        query: Vec::new(),
        header: Vec::new(),
        json: request
            .body
            .map(|body| serde_json::to_string(&body))
            .transpose()
            .context("failed to encode API request")?,
        json_file: None,
        image: None,
    })
}

fn word_new_request(args: WordNewArgs) -> Result<RequestArgs> {
    let language = path_segment("--in", &args.language)?;
    let definitions: Vec<Value> = args
        .definitions
        .into_iter()
        .map(|definition| json!({ "definition": definition }))
        .collect();
    let body = json!({
        "word": args.word,
        "word_class": args.word_class,
        "definitions": definitions,
        "ipa": args.ipa,
        "notes": args.notes,
        "categories": args.categories,
    });

    Ok(RequestArgs {
        method: Method::POST.to_string(),
        path: format!("languages/{language}/words"),
        query: Vec::new(),
        header: Vec::new(),
        json: Some(serde_json::to_string(&body).context("failed to encode word request")?),
        json_file: None,
        image: None,
    })
}

fn word_list_request(args: WordListArgs) -> Result<RequestArgs> {
    let language = path_segment("--in", &args.language)?;
    let mut query = Vec::new();
    insert_query(&mut query, "q", args.q);
    insert_query(&mut query, "exact_slug", args.exact_slug);
    insert_query(&mut query, "word_class", args.word_class);
    insert_query(&mut query, "created_before", args.created_before);
    insert_query(&mut query, "created_after", args.created_after);
    for category in args.categories {
        query.push(KeyValue {
            key: "categories[]".to_owned(),
            value: category,
        });
    }
    insert_query(
        &mut query,
        "offset",
        args.offset.map(|value| value.to_string()),
    );
    insert_query(
        &mut query,
        "limit",
        args.limit.map(|value| value.to_string()),
    );

    Ok(RequestArgs {
        method: Method::GET.to_string(),
        path: format!("languages/{language}/words"),
        query,
        header: Vec::new(),
        json: None,
        json_file: None,
        image: None,
    })
}

fn word_edit_request(args: WordEditArgs) -> Result<RequestArgs> {
    let mut body = serde_json::Map::new();
    insert_json_value(&mut body, "word", args.word);
    insert_json_value(&mut body, "word_class", args.word_class);
    insert_json_value(&mut body, "ipa", args.ipa);
    insert_json_value(&mut body, "notes", args.notes);
    if args.clear_ipa {
        body.insert("ipa".to_owned(), Value::Null);
    }
    if args.clear_notes {
        body.insert("notes".to_owned(), Value::Null);
    }
    if let Some(categories) = args.categories {
        body.insert("categories".to_owned(), json!(categories));
    }
    if args.clear_categories {
        body.insert("categories".to_owned(), json!([]));
    }
    if let Some(extra) = args.extra {
        let extra = serde_json::from_str(&extra).context("--extra must contain valid JSON")?;
        body.insert("extra".to_owned(), extra);
    }
    if args.clear_extra {
        body.insert("extra".to_owned(), Value::Null);
    }
    if body.is_empty() {
        bail!("word edit requires at least one field to update");
    }
    word_item_request("PUT", args.locator, Some(body.into()))
}

fn word_item_request(
    method: &'static str,
    args: WordLocatorArgs,
    body: Option<Value>,
) -> Result<RequestArgs> {
    let language = path_segment("--in", &args.language)?;
    let slug = path_segment("--slug", &args.slug)?;
    request_from_parts(
        method,
        format!("languages/{language}/words/{slug}/{}", args.lemma),
        body,
    )
}

fn request_from_parts(method: &str, path: String, body: Option<Value>) -> Result<RequestArgs> {
    Ok(RequestArgs {
        method: method.to_owned(),
        path,
        query: Vec::new(),
        header: Vec::new(),
        json: body
            .map(|body| serde_json::to_string(&body))
            .transpose()
            .context("failed to encode word request")?,
        json_file: None,
        image: None,
    })
}

fn insert_query(query: &mut Vec<KeyValue>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        query.push(KeyValue {
            key: key.to_owned(),
            value,
        });
    }
}

fn insert_json_value(body: &mut serde_json::Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        body.insert(key.to_owned(), json!(value));
    }
}

fn path_segment<'a>(argument: &str, value: &'a str) -> Result<&'a str> {
    if value.is_empty()
        || value
            .chars()
            .any(|character| matches!(character, '/' | '?' | '#'))
    {
        bail!("{argument} must be a single non-empty path segment");
    }
    Ok(value)
}

fn parse_base_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input).context("invalid --base-url")?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("--base-url must use http or https");
    }
    if url.host_str().is_none() {
        bail!("--base-url must include a host");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("--base-url must not contain credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("--base-url must not contain a query string or fragment");
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn parse_web_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input).context("invalid --web-url")?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("--web-url must use http or https");
    }
    if url.host_str().is_none() {
        bail!("--web-url must include a host");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("--web-url must not contain credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("--web-url must not contain a query string or fragment");
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn derive_web_url(api_url: &Url) -> Option<Url> {
    let mut url = api_url.clone();
    let path = url.path().trim_end_matches('/');
    let site_path = path.strip_suffix("/api")?;
    url.set_path(&format!("{site_path}/"));
    Some(url)
}

fn load_axm_config() -> Result<AxmConfig> {
    let Some(path) = axm_config_path() else {
        return Ok(AxmConfig::default());
    };
    if !path.exists() {
        return Ok(AxmConfig::default());
    }

    let contents = fs::read(&path)
        .with_context(|| format!("failed to read axm config file {}", path.display()))?;
    parse_axm_config(&path, &contents)
}

fn parse_axm_config(path: &Path, contents: &[u8]) -> Result<AxmConfig> {
    let mut config: AxmConfig = serde_json::from_slice(contents)
        .with_context(|| format!("invalid JSON in axm config file {}", path.display()))?;
    if let Some(token_file) = &config.token_file
        && token_file.is_relative()
    {
        let directory = path
            .parent()
            .context("axm config file must have a parent directory")?;
        config.token_file = Some(directory.join(token_file));
    }
    if let Some(language) = &config.default_language {
        path_segment("default_language in axm config", language)?;
    }
    Ok(config)
}

fn axm_config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("AXM_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let directory = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(directory.join("axm").join("config.json"))
}

fn set_configured_default_language(config: &AxmConfig) -> Result<()> {
    if env::var_os(DEFAULT_LANGUAGE_ENV).is_some() {
        return Ok(());
    }
    let Some(language) = &config.default_language else {
        return Ok(());
    };
    path_segment("default_language in axm config", language)?;

    // SAFETY: this runs before `run_main` creates Tokio's runtime or any other
    // thread, and no subsequent code mutates this environment variable.
    unsafe { env::set_var(DEFAULT_LANGUAGE_ENV, language) };
    Ok(())
}

fn load_token(token: Option<String>, token_file: Option<&Path>) -> Result<Option<String>> {
    match (token, token_file) {
        (Some(token), None) => Ok(Some(nonempty_token(token)?)),
        (None, Some(path)) => {
            if path == Path::new("-") {
                bail!(
                    "--token-file does not accept -; use AXISMUNDI_API_TOKEN for stdin-safe automation"
                );
            }
            let token = fs::read_to_string(path)
                .with_context(|| format!("failed to read token file {}", path.display()))?;
            Ok(Some(nonempty_token(token)?))
        }
        (None, None) => Ok(None),
        (Some(_), Some(_)) => bail!("use either --token or --token-file, not both"),
    }
}

fn nonempty_token(token: String) -> Result<String> {
    let token = token.trim().to_owned();
    if token.is_empty() {
        bail!("API token must not be empty");
    }
    Ok(token)
}

fn ensure_token_transport_is_safe(config: &ClientConfig) -> Result<()> {
    if config.token.is_none()
        || config.allow_insecure_http
        || config.base_url.scheme() == "https"
        || is_loopback_host(&config.base_url)
    {
        return Ok(());
    }
    bail!(
        "refusing to send an API token over non-loopback HTTP; use HTTPS or pass --allow-insecure-http"
    );
}

fn is_loopback_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn build_client(config: &ClientConfig) -> Result<Client> {
    let redirect = if config.follow_redirects {
        Policy::limited(10)
    } else {
        Policy::none()
    };
    let mut builder = Client::builder()
        .redirect(redirect)
        .user_agent(format!("axismundi-api/{}", env!("CARGO_PKG_VERSION")));
    if let Some(timeout) = config.timeout {
        builder = builder.timeout(timeout);
    }
    builder.build().context("failed to create HTTP client")
}

async fn build_request(
    client: &Client,
    config: &ClientConfig,
    args: &RequestArgs,
) -> Result<reqwest::Request> {
    let method = Method::from_bytes(args.method.as_bytes())
        .with_context(|| format!("invalid HTTP method {:?}", args.method))?;
    let url = resolve_url(&config.base_url, &args.path, &args.query)?;
    let mut headers = HeaderMap::new();
    for header in &args.header {
        validate_user_header(&header.name)?;
        headers.append(header.name.clone(), header.value.clone());
    }

    if config.token.is_some() && headers.contains_key(AUTHORIZATION) {
        bail!("--token/--token-file cannot be combined with an Authorization header");
    }

    let has_generated_body =
        args.json.is_some() || args.json_file.is_some() || args.image.is_some();
    if has_generated_body && headers.contains_key(CONTENT_TYPE) {
        bail!("do not provide Content-Type with --body, --json-file, or --image");
    }

    let mut request = client.request(method, url).headers(headers);
    if let Some(token) = &config.token {
        request = request.bearer_auth(token);
    }
    if let Some(json) = &args.json {
        request = request
            .header(CONTENT_TYPE, "application/json")
            .body(parse_json_body(json.as_bytes(), "--body")?);
    } else if let Some(path) = &args.json_file {
        let body = read_json_body(path)?;
        request = request.header(CONTENT_TYPE, "application/json").body(body);
    } else if let Some(path) = &args.image {
        request = request.multipart(image_form(path).await?);
    }
    request.build().context("failed to build request")
}

fn resolve_url(base_url: &Url, path: &str, query: &[KeyValue]) -> Result<Url> {
    if path.starts_with("//") {
        bail!("PATH must not be scheme-relative");
    }
    if path.contains('#') {
        bail!("PATH must not contain a fragment");
    }
    let path = path.strip_prefix('/').unwrap_or(path);
    if Url::parse(path).is_ok() {
        bail!("PATH must be relative to --base-url, not an absolute URL");
    }
    let path_without_query = path.split('?').next().unwrap_or_default();
    if path_without_query.split('/').any(|segment| segment == "..") {
        bail!("PATH must not contain .. segments");
    }

    let mut url = base_url.join(path).context("invalid PATH")?;
    {
        let mut pairs = url.query_pairs_mut();
        for pair in query {
            pairs.append_pair(&pair.key, &pair.value);
        }
    }
    Ok(url)
}

fn parse_key_value(input: &str) -> Result<KeyValue, String> {
    let Some((key, value)) = input.split_once('=') else {
        return Err("expected KEY=VALUE".to_owned());
    };
    if key.is_empty() {
        return Err("query parameter name must not be empty".to_owned());
    }
    Ok(KeyValue {
        key: key.to_owned(),
        value: value.to_owned(),
    })
}

fn parse_header(input: &str) -> Result<Header, String> {
    let Some((name, value)) = input.split_once(':') else {
        return Err("expected NAME: VALUE".to_owned());
    };
    let name = HeaderName::from_bytes(name.trim().as_bytes())
        .map_err(|error| format!("invalid header name: {error}"))?;
    let value = HeaderValue::from_str(value.trim())
        .map_err(|error| format!("invalid header value: {error}"))?;
    Ok(Header { name, value })
}

fn validate_user_header(name: &HeaderName) -> Result<()> {
    if [HOST, CONTENT_LENGTH, TRANSFER_ENCODING].contains(name) {
        bail!("header {name} is managed by the HTTP client and cannot be set");
    }
    Ok(())
}

fn parse_json_body(body: &[u8], source: &str) -> Result<Vec<u8>> {
    serde_json::from_slice::<Value>(body)
        .with_context(|| format!("{source} must contain valid JSON"))?;
    Ok(body.to_vec())
}

fn read_json_body(path: &Path) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    if path == Path::new("-") {
        io::stdin()
            .read_to_end(&mut body)
            .context("failed to read JSON from standard input")?;
    } else {
        body = fs::read(path)
            .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    }
    parse_json_body(&body, "--json-file")
}

async fn image_form(path: &Path) -> Result<Form> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("failed to read image {}", path.display()))?;
    let filename = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow::anyhow!("image path must include a UTF-8 filename"))?;
    let part = Part::bytes(bytes)
        .file_name(filename.to_owned())
        .mime_str(image_mime(path))
        .context("failed to set image MIME type")?;
    Ok(Form::new().part("image", part))
}

fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

fn write_response(
    response: &ResponseBody,
    mode: OutputMode,
    request: &RequestArgs,
    stdout_is_terminal: bool,
    web_url: Option<&Url>,
) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let should_format = match mode {
        OutputMode::Raw => false,
        OutputMode::Auto => is_json_content_type(response.content_type.as_deref()),
        OutputMode::Pretty => {
            if !is_json_content_type(response.content_type.as_deref()) {
                eprintln!("warning: response is not JSON; writing raw bytes");
                false
            } else {
                true
            }
        }
    };

    if should_format && !response.bytes.is_empty() {
        match serde_json::from_slice::<Value>(&response.bytes) {
            Ok(json) => {
                if mode == OutputMode::Pretty && stdout_is_terminal {
                    if let Some(output) = render_word_list(
                        &json,
                        request,
                        Utc::now(),
                        use_color(),
                        terminal_hyperlinks_supported().then_some(web_url).flatten(),
                    ) {
                        stdout
                            .write_all(output.as_bytes())
                            .context("failed to write word list response")?;
                        return Ok(());
                    }
                }
                serde_json::to_writer_pretty(&mut stdout, &json)
                    .context("failed to write JSON response")?;
                writeln!(stdout).context("failed to finish JSON response")?;
                return Ok(());
            }
            Err(error) => {
                eprintln!(
                    "warning: JSON response could not be parsed ({error}); writing raw bytes"
                );
            }
        }
    }
    stdout
        .write_all(&response.bytes)
        .context("failed to write response body")?;
    Ok(())
}

fn render_word_list(
    json: &Value,
    request: &RequestArgs,
    now: DateTime<Utc>,
    color: bool,
    web_url: Option<&Url>,
) -> Option<String> {
    let language = word_list_language(request)?;
    let object = json.as_object()?;
    let items = object.get("items")?.as_array()?;
    let total = object.get("total")?.as_i64()?;
    let offset = object.get("offset")?.as_i64()?;
    let limit = object.get("limit")?.as_i64()?.max(1);
    let has_more = object.get("has_more")?.as_bool()?;
    let query = request
        .query
        .iter()
        .find(|entry| entry.key == "q")
        .map(|entry| entry.value.as_str());

    let mut output = match query.filter(|query| !query.is_empty()) {
        Some(query) => format!("Results for searching {query:?} in `{language}`:\n"),
        None => format!("Words in `{language}`:\n"),
    };

    if items.is_empty() {
        output.push_str("\n  No words found.\n");
    }
    for (index, item) in items.iter().enumerate() {
        let word = item.get("word")?.as_str()?.trim();
        let slug = item.get("slug")?.as_str()?;
        let lemma = item.get("lemma")?.as_i64()?;
        let word_class = item
            .get("word_class_abbreviation")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(|value| format!(" ({value}.)"))
            .unwrap_or_default();
        let lemma_count = item
            .get("lemma_count")
            .and_then(Value::as_i64)
            .filter(|count| *count > 1)
            .map(|count| format!(" /{count}"))
            .unwrap_or_default();
        output.push_str(&format!(
            "\n  {} {}{}{}\n",
            paint(&format!("({})", index + 1), Style::Dim, color),
            terminal_hyperlink(
                web_url.and_then(|url| word_page_url(url, &language, slug, lemma)),
                &paint(word, Style::BoldCyan, color)
            ),
            paint(&word_class, Style::Cyan, color),
            paint(&lemma_count, Style::Dim, color),
        ));

        for (number, definition) in item
            .get("preview_definitions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|definition| !definition.is_empty())
            .enumerate()
        {
            output.push_str(&format!(
                "      {}. {}\n",
                number + 1,
                truncate_text(definition, 88)
            ));
        }

        if let (Some(author), Some(created_at)) = (
            item.get("created_by").and_then(Value::as_str),
            item.get("created_at").and_then(Value::as_str),
        ) {
            if let Ok(created_at) = DateTime::parse_from_rfc3339(created_at) {
                output.push_str(&format!(
                    "  {}\n",
                    paint(
                        &format!(
                            "added by {} {}",
                            author.trim(),
                            relative_time(created_at.with_timezone(&Utc), now)
                        ),
                        Style::Dim,
                        color,
                    )
                ));
            }
        }
    }

    let page = (offset / limit) + 1;
    let pages = if total == 0 {
        0
    } else {
        (total + limit - 1) / limit
    };
    output.push_str(&format!("\nPage {page} of {pages} ({total} items)\n"));
    if has_more {
        let mut command = format!("axm word list --in {language}");
        if let Some(query) = query.filter(|query| !query.is_empty()) {
            command.push_str(&format!(" --q {}", shell_quote(query)));
        }
        command.push_str(&format!(" --offset {}", offset + limit));
        output.push_str(&format!("Next page: {command}\n"));
    }
    Some(output)
}

fn word_list_language(request: &RequestArgs) -> Option<String> {
    let path: Vec<_> = request.path.trim_matches('/').split('/').collect();
    (path.len() == 3 && path[0] == "languages" && path[2] == "words").then(|| path[1].to_owned())
}

fn word_page_url(web_url: &Url, language: &str, slug: &str, lemma: i64) -> Option<Url> {
    let mut url = web_url.clone();
    let mut segments = url.path_segments_mut().ok()?;
    segments.pop_if_empty();
    segments.push("languages");
    segments.push(language);
    segments.push("words");
    segments.push(slug);
    segments.push(&lemma.to_string());
    drop(segments);
    Some(url)
}

fn terminal_hyperlink(url: Option<Url>, text: &str) -> String {
    let Some(url) = url else {
        return text.to_owned();
    };
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

fn terminal_hyperlinks_supported() -> bool {
    supports_hyperlinks::supports_hyperlinks() || tmux_hyperlinks_supported()
}

fn tmux_hyperlinks_supported() -> bool {
    if env::var_os("TMUX").is_none() {
        return false;
    }
    let Ok(output) = ProcessCommand::new("tmux")
        .args(["show-options", "-gqv", "terminal-features"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8(output.stdout)
            .is_ok_and(|features| tmux_terminal_features_enable_hyperlinks(&features))
}

fn tmux_terminal_features_enable_hyperlinks(features: &str) -> bool {
    features.lines().any(|entry| {
        entry
            .split(':')
            .skip(1)
            .any(|feature| feature.trim() == "hyperlinks")
    })
}

fn relative_time(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let elapsed = now.signed_duration_since(then);
    if elapsed < ChronoDuration::seconds(45) {
        "just now".to_owned()
    } else if elapsed < ChronoDuration::minutes(90) {
        let minutes = elapsed.num_minutes();
        format!(
            "{minutes} minute{} ago",
            if minutes == 1 { "" } else { "s" }
        )
    } else if elapsed < ChronoDuration::hours(36) {
        let hours = elapsed.num_hours();
        format!("{hours} hour{} ago", if hours == 1 { "" } else { "s" })
    } else {
        let days = elapsed.num_days().max(1);
        format!("{days} day{} ago", if days == 1 { "" } else { "s" })
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut characters = value.chars();
    let truncated: String = characters.by_ref().take(max_chars).collect();
    if characters.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\\"'\\\"'"))
}

fn use_color() -> bool {
    env::var_os("NO_COLOR").is_none()
}

#[derive(Clone, Copy)]
enum Style {
    BoldCyan,
    Cyan,
    Dim,
}

fn paint(value: &str, style: Style, enabled: bool) -> String {
    if !enabled || value.is_empty() {
        return value.to_owned();
    }
    let code = match style {
        Style::BoldCyan => "1;36",
        Style::Cyan => "36",
        Style::Dim => "90",
    };
    format!("\x1b[{code}m{value}\x1b[0m")
}

fn is_json_content_type(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else {
        return false;
    };
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json" || media_type.ends_with("+json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        body::Bytes,
        http::{HeaderMap as AxumHeaderMap, StatusCode},
        response::Redirect,
        routing::{get, post},
    };

    const TEST_LOCAL_BASE_URL: &str = "http://localhost:3000/api";

    fn config(base_url: &str) -> ClientConfig {
        ClientConfig {
            base_url: parse_base_url(base_url).unwrap(),
            web_url: derive_web_url(&parse_base_url(base_url).unwrap()),
            token: Some("test-token".to_owned()),
            timeout: Some(Duration::from_secs(1)),
            follow_redirects: false,
            allow_insecure_http: false,
        }
    }

    fn request_args(path: &str) -> RequestArgs {
        RequestArgs {
            method: "GET".to_owned(),
            path: path.to_owned(),
            query: Vec::new(),
            header: Vec::new(),
            json: None,
            json_file: None,
            image: None,
        }
    }

    #[test]
    fn axm_config_parses_and_resolves_a_relative_token_file() {
        let config = parse_axm_config(
            Path::new("/tmp/axm/config.json"),
            br#"{
                "api_url": "https://example.test/api",
                "web_url": "https://www.example.test",
                "token_file": "token",
                "default_language": "pas"
            }"#,
        )
        .unwrap();

        assert_eq!(config.api_url.as_deref(), Some("https://example.test/api"));
        assert_eq!(config.web_url.as_deref(), Some("https://www.example.test"));
        assert_eq!(
            config.token_file.as_deref(),
            Some(Path::new("/tmp/axm/token"))
        );
        assert_eq!(config.default_language.as_deref(), Some("pas"));
    }

    #[test]
    fn base_url_keeps_its_api_path() {
        let base_url = parse_base_url("https://example.test/api").unwrap();
        let url = resolve_url(
            &base_url,
            "/languages",
            &[parse_key_value("categories[]=noun").unwrap()],
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.test/api/languages?categories%5B%5D=noun"
        );
    }

    #[test]
    fn derives_the_website_url_from_an_api_root() {
        assert_eq!(
            derive_web_url(&parse_base_url("https://example.test/api").unwrap())
                .unwrap()
                .as_str(),
            "https://example.test/"
        );
        assert!(derive_web_url(&parse_base_url("https://example.test/v1").unwrap()).is_none());
    }

    #[test]
    fn word_page_urls_escape_path_segments() {
        assert_eq!(
            word_page_url(
                &parse_web_url("https://example.test/axis").unwrap(),
                "pas",
                "p'elar space",
                2,
            )
            .unwrap()
            .as_str(),
            "https://example.test/axis/languages/pas/words/p'elar%20space/2"
        );
    }

    #[test]
    fn tmux_feature_detection_requires_an_enabled_hyperlink_feature() {
        assert!(tmux_terminal_features_enable_hyperlinks("*:hyperlinks"));
        assert!(tmux_terminal_features_enable_hyperlinks(
            "xterm*:RGB:hyperlinks\nfoot*:hyperlinks"
        ));
        assert!(!tmux_terminal_features_enable_hyperlinks("*:hyperlinks@"));
        assert!(!tmux_terminal_features_enable_hyperlinks("hyperlinks"));
    }

    #[test]
    fn absolute_and_scheme_relative_paths_are_rejected() {
        let base_url = parse_base_url(TEST_LOCAL_BASE_URL).unwrap();
        assert!(resolve_url(&base_url, "https://elsewhere.test/api", &[]).is_err());
        assert!(resolve_url(&base_url, "//elsewhere.test/api", &[]).is_err());
        assert!(resolve_url(&base_url, "../users", &[]).is_err());
    }

    #[test]
    fn unsafe_token_transport_needs_an_override() {
        let remote_config = config("http://example.test/api");
        assert!(ensure_token_transport_is_safe(&remote_config).is_err());
        let local = config(TEST_LOCAL_BASE_URL);
        assert!(ensure_token_transport_is_safe(&local).is_ok());
    }

    #[test]
    fn recognizes_json_media_types() {
        assert!(is_json_content_type(Some("application/json")));
        assert!(is_json_content_type(Some(
            "application/problem+json; charset=utf-8"
        )));
        assert!(!is_json_content_type(Some("image/svg+xml")));
        assert!(!is_json_content_type(None));
    }

    #[test]
    fn automatic_output_is_pretty_only_for_interactive_stdout() {
        assert_eq!(
            output_mode(OutputMode::Auto, false, true),
            OutputMode::Pretty
        );
        assert_eq!(output_mode(OutputMode::Auto, false, false), OutputMode::Raw);
        assert_eq!(
            output_mode(OutputMode::Pretty, false, false),
            OutputMode::Pretty
        );
        assert_eq!(output_mode(OutputMode::Auto, true, true), OutputMode::Raw);
    }

    #[test]
    fn word_lists_render_as_compact_terminal_output() {
        let request = RequestArgs {
            method: "GET".to_owned(),
            path: "languages/pas/words".to_owned(),
            query: vec![KeyValue {
                key: "q".to_owned(),
                value: "a".to_owned(),
            }],
            header: Vec::new(),
            json: None,
            json_file: None,
            image: None,
        };
        let now = DateTime::parse_from_rfc3339("2026-08-29T12:01:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let response = json!({
            "items": [{
                "word": "p'elar",
                "slug": "p'elar",
                "lemma": 1,
                "word_class_abbreviation": "n",
                "lemma_count": 2,
                "preview_definitions": ["blah", "foo"],
                "created_by": "autumn",
                "created_at": "2026-08-29T12:00:00Z"
            }, {
                "word": "asdj",
                "slug": "asdj",
                "lemma": 1,
                "word_class_abbreviation": "v",
                "preview_definitions": ["asd"]
            }],
            "total": 12,
            "offset": 0,
            "limit": 10,
            "has_more": true
        });

        assert_eq!(
            render_word_list(&response, &request, now, false, None).unwrap(),
            concat!(
                "Results for searching \"a\" in `pas`:\n\n",
                "  (1) p'elar (n.) /2\n",
                "      1. blah\n",
                "      2. foo\n",
                "  added by autumn 1 minute ago\n\n",
                "  (2) asdj (v.)\n",
                "      1. asd\n\n",
                "Page 1 of 2 (12 items)\n",
                "Next page: axm word list --in pas --q 'a' --offset 10\n",
            )
        );
    }

    #[test]
    fn word_lists_link_word_forms_when_a_web_url_is_available() {
        let request = request_args("languages/pas/words");
        let response = json!({
            "items": [{
                "word": "p'elar",
                "slug": "p'elar",
                "lemma": 1,
                "preview_definitions": []
            }],
            "total": 1,
            "offset": 0,
            "limit": 10,
            "has_more": false
        });

        let output = render_word_list(
            &response,
            &request,
            Utc::now(),
            false,
            Some(&parse_web_url("https://example.test").unwrap()),
        )
        .unwrap();

        assert!(output.contains(
            "\x1b]8;;https://example.test/languages/pas/words/p'elar/1\x1b\\p'elar\x1b]8;;\x1b\\"
        ));
    }

    #[test]
    fn json_flag_requests_raw_output() {
        let cli = Cli::try_parse_from(["axm", "--json", "request", "GET", "/languages"]).unwrap();

        assert!(cli.raw_json);
        assert_eq!(cli.output, OutputMode::Auto);
    }

    #[test]
    fn rate_limit_delay_uses_retry_after_then_bounded_backoff() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(rate_limit_delay(&headers, 0), Duration::from_secs(7));

        headers.clear();
        assert_eq!(rate_limit_delay(&headers, 0), Duration::from_secs(1));
        assert_eq!(rate_limit_delay(&headers, 3), Duration::from_secs(8));
        assert_eq!(rate_limit_delay(&headers, 8), Duration::from_secs(30));
    }

    #[test]
    fn clap_rejects_multiple_body_options() {
        assert!(
            Cli::try_parse_from([
                "axismundi-api",
                "request",
                "POST",
                "/users",
                "--body",
                "{}",
                "--image",
                "avatar.png",
            ])
            .is_err()
        );
    }

    #[test]
    fn word_new_command_builds_the_current_word_create_payload() {
        let cli = Cli::try_parse_from([
            "axm",
            "word",
            "new",
            "--in",
            "pas",
            "--word",
            "kok'ebe",
            "--def",
            "to stink",
            "--class",
            "v",
            "--def",
            "to smell bad",
        ])
        .unwrap();
        let Command::Word { command } = cli.command else {
            panic!("expected word command");
        };
        let request = word_command_request(command).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "languages/pas/words");
        assert_eq!(
            serde_json::from_str::<Value>(request.json.as_deref().unwrap()).unwrap(),
            json!({
                "word": "kok'ebe",
                "word_class": "v",
                "definitions": [
                    { "definition": "to stink" },
                    { "definition": "to smell bad" }
                ],
                "ipa": null,
                "notes": null,
                "categories": []
            })
        );
    }

    #[test]
    fn word_list_builds_the_language_search_request() {
        let request = word_list_request(WordListArgs {
            language: "pas".to_owned(),
            q: Some("stink smell".to_owned()),
            exact_slug: None,
            word_class: Some("v".to_owned()),
            created_before: None,
            created_after: None,
            categories: vec!["informal".to_owned(), "taboo".to_owned()],
            offset: Some(20),
            limit: Some(10),
        })
        .unwrap();

        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "languages/pas/words");
        let query: Vec<_> = request
            .query
            .iter()
            .map(|entry| (entry.key.as_str(), entry.value.as_str()))
            .collect();
        assert_eq!(
            query,
            vec![
                ("q", "stink smell"),
                ("word_class", "v"),
                ("categories[]", "informal"),
                ("categories[]", "taboo"),
                ("offset", "20"),
                ("limit", "10"),
            ]
        );
    }

    #[test]
    fn word_edit_updates_only_requested_fields_and_can_clear_values() {
        let request = word_edit_request(WordEditArgs {
            locator: WordLocatorArgs {
                language: "pas".to_owned(),
                slug: "kokebe".to_owned(),
                lemma: 2,
            },
            word: Some("kok'ebe".to_owned()),
            word_class: None,
            ipa: None,
            clear_ipa: true,
            notes: None,
            clear_notes: false,
            categories: None,
            clear_categories: true,
            extra: Some(r#"{"source":"field notes"}"#.to_owned()),
            clear_extra: false,
        })
        .unwrap();

        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "languages/pas/words/kokebe/2");
        assert_eq!(
            serde_json::from_str::<Value>(request.json.as_deref().unwrap()).unwrap(),
            json!({
                "word": "kok'ebe",
                "ipa": null,
                "categories": [],
                "extra": { "source": "field notes" },
            })
        );
    }

    #[tokio::test]
    async fn request_contains_auth_query_and_json_body() {
        let client = build_client(&config(TEST_LOCAL_BASE_URL)).unwrap();
        let mut args = request_args("/users");
        args.method = "POST".to_owned();
        args.query = vec![
            parse_key_value("tag=one").unwrap(),
            parse_key_value("tag=two").unwrap(),
        ];
        args.json = Some(r#"{"username":"test"}"#.to_owned());

        let request = build_request(&client, &config(TEST_LOCAL_BASE_URL), &args)
            .await
            .unwrap();
        assert_eq!(request.method(), Method::POST);
        assert_eq!(
            request.url().as_str(),
            "http://localhost:3000/api/users?tag=one&tag=two"
        );
        assert_eq!(
            request.headers().get(AUTHORIZATION).unwrap(),
            "Bearer test-token"
        );
        assert_eq!(
            request.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    async fn echo(headers: AxumHeaderMap, body: Bytes) -> Json<Value> {
        Json(json!({
            "authorization": headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
            "body": String::from_utf8_lossy(&body),
        }))
    }

    async fn definition_preview() -> Json<Value> {
        Json(json!({
            "items": [
                { "definition": "first gloss" },
                { "definition": "second gloss" }
            ]
        }))
    }

    async fn spawn_server() -> (Url, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/api/echo", post(echo))
            .route("/api/languages/pas/words", post(echo))
            .route(
                "/api/languages/pas/words/pelar/1/definitions",
                get(definition_preview),
            )
            .route("/api/redirect", get(|| async { Redirect::to("/api/echo") }))
            .route(
                "/api/status",
                get(|| async { (StatusCode::BAD_REQUEST, "bad request") }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (
            Url::parse(&format!("http://{address}/api")).unwrap(),
            handle,
        )
    }

    #[tokio::test]
    async fn client_sends_auth_and_does_not_follow_redirects() {
        let (base_url, server) = spawn_server().await;
        let config = ClientConfig {
            base_url: parse_base_url(base_url.as_str()).unwrap(),
            web_url: None,
            token: Some("test-token".to_owned()),
            timeout: Some(Duration::from_secs(1)),
            follow_redirects: false,
            allow_insecure_http: false,
        };
        let client = build_client(&config).unwrap();
        let mut args = request_args("echo");
        args.method = "POST".to_owned();
        args.json = Some(r#"{"word":"pater"}"#.to_owned());
        let response = client
            .execute(build_request(&client, &config, &args).await.unwrap())
            .await
            .unwrap();
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["authorization"], "Bearer test-token");
        assert_eq!(body["body"], r#"{"word":"pater"}"#);

        let response = client
            .get(resolve_url(&config.base_url, "redirect", &[]).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        server.abort();
    }

    #[tokio::test]
    async fn word_list_loads_definitions_when_the_api_has_no_previews() {
        let (base_url, server) = spawn_server().await;
        let config = ClientConfig {
            base_url: parse_base_url(base_url.as_str()).unwrap(),
            web_url: None,
            token: None,
            timeout: Some(Duration::from_secs(1)),
            follow_redirects: false,
            allow_insecure_http: false,
        };
        let client = build_client(&config).unwrap();
        let mut response = ResponseBody {
            status: StatusCode::OK,
            content_type: Some("application/json".to_owned()),
            bytes: serde_json::to_vec(&json!({
                "items": [{ "slug": "pelar", "lemma": 1 }]
            }))
            .unwrap(),
        };

        enrich_word_list_with_definitions(&client, &config, &mut response, "pas", 0)
            .await
            .unwrap();

        let body: Value = serde_json::from_slice(&response.bytes).unwrap();
        assert_eq!(
            body["items"][0]["preview_definitions"],
            json!(["first gloss", "second gloss"])
        );
        server.abort();
    }

    #[tokio::test]
    async fn word_new_sends_the_resource_oriented_request() {
        let (base_url, server) = spawn_server().await;
        let config = ClientConfig {
            base_url: parse_base_url(base_url.as_str()).unwrap(),
            web_url: None,
            token: Some("test-token".to_owned()),
            timeout: Some(Duration::from_secs(1)),
            follow_redirects: false,
            allow_insecure_http: false,
        };
        let request = word_new_request(WordNewArgs {
            language: "pas".to_owned(),
            word: "kok'ebe".to_owned(),
            definitions: vec!["to stink".to_owned()],
            word_class: "v".to_owned(),
            ipa: None,
            notes: None,
            categories: Vec::new(),
        })
        .unwrap();
        let client = build_client(&config).unwrap();
        let response = client
            .execute(build_request(&client, &config, &request).await.unwrap())
            .await
            .unwrap();
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["authorization"], "Bearer test-token");
        assert_eq!(
            serde_json::from_str::<Value>(body["body"].as_str().unwrap()).unwrap(),
            json!({
                "word": "kok'ebe",
                "word_class": "v",
                "definitions": [{ "definition": "to stink" }],
                "ipa": null,
                "notes": null,
                "categories": []
            })
        );
        server.abort();
    }

    #[tokio::test]
    async fn non_success_response_body_is_preserved() {
        let (base_url, server) = spawn_server().await;
        let base_url = parse_base_url(base_url.as_str()).unwrap();
        let client = build_client(&ClientConfig {
            base_url: base_url.clone(),
            web_url: None,
            token: None,
            timeout: Some(Duration::from_secs(1)),
            follow_redirects: false,
            allow_insecure_http: false,
        })
        .unwrap();
        let response = client
            .get(resolve_url(&base_url, "status", &[]).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.text().await.unwrap(), "bad request");
        server.abort();
    }
}
