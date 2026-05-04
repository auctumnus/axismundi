use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use axum_extra::extract::Multipart;
use serde::Serialize;

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    err::{AppError, AppResult, bad_request, forbidden},
    model::{
        audit_log::{
            AuditActionType, AuditLogRepository, AuditableResource, CreateAuditLog,
        },
        definitions::{CreateDefinition, DefinitionRepository},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_categories::WordCategoryRepository,
        word_classes::WordClassRepository,
        words::{CreateWord, WordRepository},
    },
    util::{AppState, extract_session::Session, s3::multipart_read_error},
};

const MAX_IMPORT_ROWS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UnknownPolicy {
    AutoCreate,
    SkipRow,
    Fail,
}

impl UnknownPolicy {
    fn from_form(s: &str) -> Self {
        match s {
            "skip_row" => Self::SkipRow,
            "fail" => Self::Fail,
            _ => Self::AutoCreate,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UnknownCategoryPolicy {
    AutoCreate,
    SkipCategory,
    Fail,
}

impl UnknownCategoryPolicy {
    fn from_form(s: &str) -> Self {
        match s {
            "skip_category" => Self::SkipCategory,
            "fail" => Self::Fail,
            _ => Self::AutoCreate,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SeparatorChoice {
    Comma,
    Tab,
    Semicolon,
}

impl SeparatorChoice {
    fn from_form(s: &str) -> Self {
        match s {
            "tab" => Self::Tab,
            "semicolon" => Self::Semicolon,
            _ => Self::Comma,
        }
    }
    fn as_byte(self) -> u8 {
        match self {
            Self::Comma => b',',
            Self::Tab => b'\t',
            Self::Semicolon => b';',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum QuoteChoice {
    Double,
    Single,
    None,
}

impl QuoteChoice {
    fn from_form(s: &str) -> Self {
        match s {
            "single" => Self::Single,
            "none" => Self::None,
            _ => Self::Double,
        }
    }
    fn as_byte(self) -> u8 {
        match self {
            Self::Single => b'\'',
            _ => b'"',
        }
    }
    fn enabled(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportOptions {
    pub separator: SeparatorChoice,
    pub quote: QuoteChoice,
    pub has_header: bool,
    pub col_word: Option<usize>,
    pub col_definition: Option<usize>,
    pub col_word_class: Option<usize>,
    pub col_word_category: Option<usize>,
    pub col_ipa: Option<usize>,
    pub col_notes: Option<usize>,
    pub on_unknown_class: UnknownPolicy,
    pub on_unknown_category: UnknownCategoryPolicy,
}

impl ImportOptions {
    fn defaults() -> Self {
        Self {
            separator: SeparatorChoice::Comma,
            quote: QuoteChoice::Double,
            has_header: false,
            col_word: Some(0),
            col_definition: Some(1),
            col_word_class: Some(2),
            col_word_category: Some(3),
            col_ipa: Some(4),
            col_notes: Some(5),
            on_unknown_class: UnknownPolicy::AutoCreate,
            on_unknown_category: UnknownCategoryPolicy::AutoCreate,
        }
    }
}

#[derive(Template)]
#[template(path = "words/import.html")]
#[allow(dead_code)]
struct ImportTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    options: ImportOptions,
    can_edit_language: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "words/import_results.html")]
#[allow(dead_code)]
struct ImportResultsTemplate {
    current_user: Option<User>,
    language: Language,
    imported: usize,
    skipped: Vec<(usize, String)>,
    total_rows: usize,
    can_edit_language: bool,
    can_delete_language: bool,
}

async fn load_common(
    state: &AppState,
    user: &User,
    language_code: &str,
) -> AppResult<(Language, bool, bool)> {
    let languages = LanguageRepository::new(state.clone());
    let permissions = LanguagePermissionRepository::new(state.clone());
    let language = languages.find_by_code(language_code).await?;
    let can_edit_language = permissions
        .can_edit_language(Some(user), &language.id)
        .await?;
    let can_delete_language = permissions
        .can_delete_language(Some(user), &language.id)
        .await?;
    if !can_edit_language {
        return Err(forbidden(
            "you don't have permission to import to this language",
        ));
    }
    Ok((language, can_edit_language, can_delete_language))
}

pub(super) async fn import_form(
    s: Session,
    State(state): State<AppState>,
    Path(language_code): Path<String>,
) -> (StatusCode, Response) {
    let Some(current_user) = s.user().cloned() else {
        return render_generic_error(s, forbidden("you must be logged in to import")).await;
    };

    let (language, can_edit_language, can_delete_language) =
        attempt!(s, load_common(&state, &current_user, &language_code).await);

    let template = ImportTemplate {
        current_user: Some(current_user),
        error: None,
        language,
        options: ImportOptions::defaults(),
        can_edit_language,
        can_delete_language,
    };
    okay(render_template(template))
}

pub(super) async fn import_submit(
    s: Session,
    State(state): State<AppState>,
    Path(language_code): Path<String>,
    mut multipart: Multipart,
) -> (StatusCode, Response) {
    let Some(current_user) = s.user().cloned() else {
        return render_generic_error(s, forbidden("you must be logged in to import")).await;
    };

    let (language, can_edit_language, can_delete_language) =
        attempt!(s, load_common(&state, &current_user, &language_code).await);

    let render_form_error = |error: AppError, options: ImportOptions| {
        let template = ImportTemplate {
            current_user: Some(current_user.clone()),
            error: Some(error),
            language: language.clone(),
            options,
            can_edit_language,
            can_delete_language,
        };
        (StatusCode::BAD_REQUEST, render_template(template))
    };

    let (file_data, opts) = match parse_multipart(&mut multipart).await {
        Ok(v) => v,
        Err(e) => return render_form_error(e, ImportOptions::defaults()),
    };

    let Some(file_data) = file_data else {
        return render_form_error(bad_request("no file uploaded"), opts);
    };
    if file_data.is_empty() {
        return render_form_error(bad_request("uploaded file is empty"), opts);
    }
    if let Err(e) = validate_options(&opts) {
        return render_form_error(e, opts);
    }

    let summary = match import_csv_bytes(&state, &current_user, &language, &file_data, &opts).await
    {
        Ok(s) => s,
        Err(e) => return render_form_error(e, opts),
    };

    let total_rows = summary.imported + summary.skipped.len();
    let template = ImportResultsTemplate {
        current_user: Some(current_user),
        language,
        imported: summary.imported,
        skipped: summary.skipped,
        total_rows,
        can_edit_language,
        can_delete_language,
    };
    okay(render_template(template))
}

#[derive(Debug)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: Vec<(usize, String)>,
}

fn validate_options(opts: &ImportOptions) -> AppResult<()> {
    if !any_columns_set(opts) {
        return Err(bad_request("at least one column must be mapped"));
    }
    if opts.col_word.is_none() {
        return Err(bad_request(
            "the word column must be mapped (it's required)",
        ));
    }
    if opts.col_definition.is_none() {
        return Err(bad_request(
            "the definition column must be mapped (it's required)",
        ));
    }
    if opts.col_word_class.is_none() {
        return Err(bad_request(
            "the word class column must be mapped (it's required)",
        ));
    }
    Ok(())
}

/// Parse `bytes` as csv per `opts` and import each row as a word + definition into `language`.
/// Per-row failures are collected in `summary.skipped` rather than aborting; only csv-level
/// errors, the row cap, or a `Fail` policy hit on an unknown class/category cause an `Err`.
///
/// On completion, emits a single audit log entry summarising the import when the
/// acting user does not have direct editor permission on the language (i.e. is
/// acting as admin/mod). Editors with direct permission generate no audit log,
/// matching the per-row semantics of regular word/definition creation.
pub async fn import_csv_bytes(
    state: &AppState,
    user: &User,
    language: &Language,
    bytes: &[u8],
    opts: &ImportOptions,
) -> AppResult<ImportSummary> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(opts.separator.as_byte())
        .quoting(opts.quote.enabled())
        .quote(opts.quote.as_byte())
        .has_headers(opts.has_header)
        .flexible(true)
        .from_reader(bytes);

    let mut imported = 0usize;
    let mut skipped: Vec<(usize, String)> = Vec::new();

    for (idx, rec_result) in reader.records().enumerate() {
        if imported + skipped.len() >= MAX_IMPORT_ROWS {
            return Err(bad_request(format!(
                "too many rows (cap is {MAX_IMPORT_ROWS}); imported {imported}, skipped {} before stopping",
                skipped.len()
            )));
        }
        let row_num = idx + 1 + usize::from(opts.has_header);
        let rec = match rec_result {
            Ok(r) => r,
            Err(e) => {
                return Err(bad_request(format!(
                    "csv parse error at row {row_num}: {e}"
                )));
            }
        };

        let parsed = match build_parsed_row(&rec, opts) {
            Ok(p) => p,
            Err(msg) => {
                skipped.push((row_num, msg));
                continue;
            }
        };

        match process_row(state, user, &language.code, language, parsed, opts).await {
            Ok(()) => imported += 1,
            Err(RowError::Skip(msg)) => skipped.push((row_num, msg)),
            Err(RowError::Fatal(e)) => return Err(e),
        }
    }

    let has_direct_edit = LanguagePermissionRepository::new(state.clone())
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await?;
    if !has_direct_edit {
        AuditLogRepository::new(state.clone())
            .create_internal(CreateAuditLog {
                user_id: Some(user.id),
                action: AuditActionType::Imported,
                resource_type: AuditableResource::Language,
                resource_id: language.id,
                details: serde_json::json!({
                    "language_id": language.id,
                    "language_code": language.code,
                    "imported": imported,
                    "skipped": skipped.len(),
                }),
            })
            .await?;
    }

    Ok(ImportSummary { imported, skipped })
}

fn any_columns_set(opts: &ImportOptions) -> bool {
    opts.col_word.is_some()
        || opts.col_definition.is_some()
        || opts.col_word_class.is_some()
        || opts.col_word_category.is_some()
        || opts.col_ipa.is_some()
        || opts.col_notes.is_some()
}

// Form is 1-indexed for users; backend stores 0-indexed offsets.
fn parse_col(s: &str) -> Option<usize> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let n: usize = t.parse().ok()?;
    n.checked_sub(1)
}

async fn parse_multipart(multipart: &mut Multipart) -> AppResult<(Option<Vec<u8>>, ImportOptions)> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut opts = ImportOptions::defaults();
    let mut has_header_seen = false;

    while let Some(field) = multipart.next_field().await.map_err(multipart_read_error)? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let bytes = field.bytes().await.map_err(multipart_read_error)?;
                file_data = Some(bytes.to_vec());
            }
            "separator" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.separator = SeparatorChoice::from_form(&v);
            }
            "quote" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.quote = QuoteChoice::from_form(&v);
            }
            "has_header" => {
                let _ = field.text().await.map_err(multipart_read_error)?;
                has_header_seen = true;
            }
            "col_word" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.col_word = parse_col(&v);
            }
            "col_definition" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.col_definition = parse_col(&v);
            }
            "col_word_class" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.col_word_class = parse_col(&v);
            }
            "col_word_category" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.col_word_category = parse_col(&v);
            }
            "col_ipa" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.col_ipa = parse_col(&v);
            }
            "col_notes" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.col_notes = parse_col(&v);
            }
            "on_unknown_class" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.on_unknown_class = UnknownPolicy::from_form(&v);
            }
            "on_unknown_category" => {
                let v = field.text().await.map_err(multipart_read_error)?;
                opts.on_unknown_category = UnknownCategoryPolicy::from_form(&v);
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    opts.has_header = has_header_seen;
    Ok((file_data, opts))
}

struct ParsedRow {
    word: String,
    definition: String,
    class_abbr: String,
    category_abbr: Option<String>,
    ipa: Option<String>,
    notes: Option<String>,
}

fn pick(rec: &csv::StringRecord, idx: Option<usize>) -> Option<String> {
    let i = idx?;
    let raw = rec.get(i)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn build_parsed_row(rec: &csv::StringRecord, opts: &ImportOptions) -> Result<ParsedRow, String> {
    let word = pick(rec, opts.col_word).ok_or_else(|| "missing word form".to_string())?;
    let definition =
        pick(rec, opts.col_definition).ok_or_else(|| "missing definition".to_string())?;
    let class_abbr =
        pick(rec, opts.col_word_class).ok_or_else(|| "missing word class".to_string())?;
    let category_abbr = pick(rec, opts.col_word_category);
    let ipa = pick(rec, opts.col_ipa);
    let notes = pick(rec, opts.col_notes);
    Ok(ParsedRow {
        word,
        definition,
        class_abbr,
        category_abbr,
        ipa,
        notes,
    })
}

enum RowError {
    Skip(String),
    Fatal(AppError),
}

async fn resolve_class(
    state: &AppState,
    user: &User,
    lang_code: &str,
    language_id: &uuid::Uuid,
    abbr: &str,
    policy: UnknownPolicy,
) -> Result<String, RowError> {
    use axum::http::StatusCode as Sc;
    let repo = WordClassRepository::new(state.clone());
    match policy {
        UnknownPolicy::AutoCreate => {
            let wc = repo
                .find_or_create_by_abbreviation(user, lang_code, abbr)
                .await
                .map_err(|e| RowError::Skip(format!("word class '{abbr}': {}", e.message)))?;
            Ok(wc.abbreviation)
        }
        UnknownPolicy::SkipRow => match repo.find_by_abbreviation(language_id, abbr).await {
            Ok(wc) => Ok(wc.abbreviation),
            Err(e) if e.status_code == Sc::NOT_FOUND => {
                Err(RowError::Skip(format!("unknown word class '{abbr}'")))
            }
            Err(e) => Err(RowError::Skip(e.message)),
        },
        UnknownPolicy::Fail => match repo.find_by_abbreviation(language_id, abbr).await {
            Ok(wc) => Ok(wc.abbreviation),
            Err(e) if e.status_code == Sc::NOT_FOUND => Err(RowError::Fatal(bad_request(format!(
                "unknown word class '{abbr}' and import is set to fail on unknowns"
            )))),
            Err(e) => Err(RowError::Fatal(e)),
        },
    }
}

async fn resolve_category(
    state: &AppState,
    user: &User,
    lang_code: &str,
    language_id: &uuid::Uuid,
    abbr: &str,
    policy: UnknownCategoryPolicy,
) -> Result<Option<String>, RowError> {
    use axum::http::StatusCode as Sc;
    let repo = WordCategoryRepository::new(state.clone());
    match policy {
        UnknownCategoryPolicy::AutoCreate => {
            let cat = repo
                .find_or_create_by_abbreviation(user, lang_code, abbr)
                .await
                .map_err(|e| RowError::Skip(format!("word category '{abbr}': {}", e.message)))?;
            Ok(Some(cat.abbreviation))
        }
        UnknownCategoryPolicy::SkipCategory => {
            match repo.find_by_abbreviation(language_id, abbr).await {
                Ok(c) => Ok(Some(c.abbreviation)),
                Err(e) if e.status_code == Sc::NOT_FOUND => Ok(None),
                Err(e) => Err(RowError::Skip(e.message)),
            }
        }
        UnknownCategoryPolicy::Fail => match repo.find_by_abbreviation(language_id, abbr).await {
            Ok(c) => Ok(Some(c.abbreviation)),
            Err(e) if e.status_code == Sc::NOT_FOUND => Err(RowError::Fatal(bad_request(format!(
                "unknown word category '{abbr}' and import is set to fail on unknowns"
            )))),
            Err(e) => Err(RowError::Fatal(e)),
        },
    }
}

async fn process_row(
    state: &AppState,
    user: &User,
    lang_code: &str,
    language: &Language,
    parsed: ParsedRow,
    opts: &ImportOptions,
) -> Result<(), RowError> {
    // Lowercase class/category abbreviations before lookup so a CSV "N" matches the default "n".
    // Auto-create also uses the lowercased form, keeping new entries consistent with axismundi's
    // default-lowercase convention.
    let class_lookup = parsed.class_abbr.to_lowercase();
    let class_abbr = resolve_class(
        state,
        user,
        lang_code,
        &language.id,
        &class_lookup,
        opts.on_unknown_class,
    )
    .await?;

    let mut categories: Vec<String> = Vec::new();
    if let Some(cat_field) = parsed.category_abbr.as_ref() {
        for piece in cat_field.split(',') {
            let trimmed = piece.trim();
            if trimmed.is_empty() {
                continue;
            }
            let cat_lookup = trimmed.to_lowercase();
            if let Some(a) = resolve_category(
                state,
                user,
                lang_code,
                &language.id,
                &cat_lookup,
                opts.on_unknown_category,
            )
            .await?
            {
                if !categories.contains(&a) {
                    categories.push(a);
                }
            }
        }
    }

    let words = WordRepository::new(state.clone());
    let defs = DefinitionRepository::new(state.clone());

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| RowError::Fatal(AppError::from(e)))?;

    let create_word = CreateWord {
        word: parsed.word,
        word_class: class_abbr,
        ipa: parsed.ipa,
        notes: parsed.notes,
        extra: None,
        categories: Some(categories),
    };

    // Silent variants: the import emits a single summary audit log entry after
    // the loop completes, and never produces user activity entries. Permission
    // is enforced once in `load_common` before we reach this point.
    let word_id = match words
        .create_with_tx_silent(user, language.id, create_word, &mut tx)
        .await
    {
        Ok(id) => id,
        Err(e) => return Err(RowError::Skip(e.message)),
    };

    if let Err(e) = defs
        .create_with_tx_silent(
            user,
            word_id,
            language.id,
            CreateDefinition {
                definition: parsed.definition,
                context: None,
            },
            &mut tx,
        )
        .await
    {
        return Err(RowError::Skip(e.message));
    }

    tx.commit()
        .await
        .map_err(|e| RowError::Skip(format!("commit error: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG;
    use crate::create_router;
    use crate::email;
    use crate::model::languages::{CreateLanguage, LanguageRepository};
    use crate::model::users::UserRepository;
    use crate::model::word_categories::WordCategoryRepository;
    use crate::model::word_classes::WordClassRepository;
    use crate::model::words::{WordRepository, WordSearch};
    use crate::pagination::PaginatedRequest;
    use sqlx::PgPool;

    const SXS: &[u8] = include_bytes!("../../../../tests/fixtures/csv/sxs.csv");
    const SNGMO: &[u8] = include_bytes!("../../../../tests/fixtures/csv/sngmo.csv");
    const YAZZ: &[u8] = include_bytes!("../../../../tests/fixtures/csv/yazz.csv");
    const NOT_A_DICTIONARY: &[u8] =
        include_bytes!("../../../../tests/fixtures/csv/not_a_dictionary.csv");

    async fn setup() -> (AppState, User, Language) {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        let username = crate::tests::random_name();
        let _token = crate::tests::make_authed_user(&username, &app, email_service.clone()).await;
        let user_id = sqlx::query_scalar!("select id from users where username = $1", username)
            .fetch_one(&pool)
            .await
            .unwrap();
        let user = UserRepository::new(app_state.clone())
            .find_by_id(user_id)
            .await
            .unwrap();

        let lang = LanguageRepository::new(app_state.clone())
            .create(
                &user,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "import test".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        (app_state, user, lang)
    }

    async fn count_words(state: &AppState, lang_id: uuid::Uuid) -> i64 {
        WordRepository::new(state.clone())
            .search(
                &lang_id,
                PaginatedRequest {
                    limit: 10_000,
                    offset: 0,
                },
                WordSearch::default(),
            )
            .await
            .unwrap()
            .total
    }

    #[tokio::test]
    async fn test_import_sxs_with_defaults_imports_three_rows() {
        let (state, user, lang) = setup().await;

        let summary = import_csv_bytes(&state, &user, &lang, SXS, &ImportOptions::defaults())
            .await
            .unwrap();

        assert_eq!(summary.imported, 3, "expected 3 imports, got {summary:?}");
        assert!(
            summary.skipped.is_empty(),
            "expected no skipped rows, got {:?}",
            summary.skipped
        );
        assert_eq!(count_words(&state, lang.id).await, 3);
    }

    #[tokio::test]
    async fn test_import_sxs_lowercases_class_to_match_default() {
        // SXS uses uppercase "N" for word class. The language has a default lowercase "n".
        // After import, no new word_class should be created — every word should reuse "n".
        let (state, user, lang) = setup().await;
        let class_count_before = WordClassRepository::new(state.clone())
            .list_all(lang.id)
            .await
            .unwrap()
            .len();

        let _ = import_csv_bytes(&state, &user, &lang, SXS, &ImportOptions::defaults())
            .await
            .unwrap();

        let classes = WordClassRepository::new(state.clone())
            .list_all(lang.id)
            .await
            .unwrap();
        assert_eq!(
            classes.len(),
            class_count_before,
            "no new word_class should have been auto-created (got {:?})",
            classes.iter().map(|c| &c.abbreviation).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_import_sxs_auto_creates_categories() {
        // SXS column 3 has "M" and "F". Lowercased to "m"/"f". No defaults exist for these.
        let (state, user, lang) = setup().await;

        let _ = import_csv_bytes(&state, &user, &lang, SXS, &ImportOptions::defaults())
            .await
            .unwrap();

        let cats = WordCategoryRepository::new(state.clone())
            .list_all(lang.id)
            .await
            .unwrap();
        let abbrs: Vec<&str> = cats.iter().map(|c| c.abbreviation.as_str()).collect();
        assert!(abbrs.contains(&"m"), "expected 'm' category, got {abbrs:?}");
        assert!(abbrs.contains(&"f"), "expected 'f' category, got {abbrs:?}");
    }

    #[tokio::test]
    async fn test_import_sngmo_handles_diverse_data() {
        // SNGMO has mixed PoS abbreviations (N/V/PPR/ADV/PTC), some quoted multi-word categories
        // like "aba,bwa", and notes containing @-references. Just verify it doesn't fatal-out.
        let (state, user, lang) = setup().await;

        let summary = import_csv_bytes(&state, &user, &lang, SNGMO, &ImportOptions::defaults())
            .await
            .unwrap();

        assert!(
            summary.imported > 0,
            "expected some imports, got {summary:?}"
        );
        // Ensure auto-create handled the unknown classes gracefully — find at least one.
        let classes = WordClassRepository::new(state.clone())
            .list_all(lang.id)
            .await
            .unwrap();
        let abbrs: Vec<&str> = classes.iter().map(|c| c.abbreviation.as_str()).collect();
        assert!(
            abbrs.contains(&"ppr") || abbrs.contains(&"ptc"),
            "expected an auto-created class like 'ppr' or 'ptc', got {abbrs:?}"
        );
    }

    #[tokio::test]
    async fn test_import_yazz_handles_seven_columns_and_multiline_cells() {
        // YAZZ exports 7 columns with etymology in column 5 and notes in column 6.
        // Cells contain newlines inside quoted fields; the csv crate must handle this.
        let (state, user, lang) = setup().await;

        let summary = import_csv_bytes(&state, &user, &lang, YAZZ, &ImportOptions::defaults())
            .await
            .unwrap();

        assert!(
            summary.imported > 50,
            "expected many imports for YAZZ, got {summary:?}"
        );
    }

    #[tokio::test]
    async fn test_import_garbage_csv_skips_all_rows_without_panic() {
        // not_a_dictionary.csv is a 2-column dataset (date, number) the user uploaded by mistake.
        // Default mapping wants column 2 (word_class) but it doesn't exist — every row should
        // skip with "missing word class" rather than crashing or partially importing.
        let (state, user, lang) = setup().await;

        let summary = import_csv_bytes(
            &state,
            &user,
            &lang,
            NOT_A_DICTIONARY,
            &ImportOptions::defaults(),
        )
        .await
        .unwrap();

        assert_eq!(summary.imported, 0, "expected 0 imports, got {summary:?}");
        assert!(!summary.skipped.is_empty(), "expected skipped rows");
        for (_, msg) in &summary.skipped {
            assert!(
                msg.contains("missing word class") || msg.contains("missing definition"),
                "every skip should be a 'missing required field' error; got {msg:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_fail_policy_aborts_on_unknown_class() {
        // SNGMO has "PPR" (not in defaults). With Fail policy, the whole import should error.
        let (state, user, lang) = setup().await;
        let mut opts = ImportOptions::defaults();
        opts.on_unknown_class = UnknownPolicy::Fail;

        let result = import_csv_bytes(&state, &user, &lang, SNGMO, &opts).await;
        assert!(result.is_err(), "expected fatal abort on unknown class");
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("unknown word class"),
            "expected 'unknown word class' in error, got {msg:?}"
        );
        // Nothing should have been imported.
        assert_eq!(count_words(&state, lang.id).await, 0);
    }

    #[tokio::test]
    async fn test_skip_row_policy_skips_unknown_class_rows() {
        // SXS uses class "N" (matches default "n"). Every row should still import.
        // Then SNGMO with SkipRow should import the rows whose class matches a default and
        // skip those whose class doesn't.
        let (state, user, lang) = setup().await;
        let mut opts = ImportOptions::defaults();
        opts.on_unknown_class = UnknownPolicy::SkipRow;

        let summary = import_csv_bytes(&state, &user, &lang, SNGMO, &opts)
            .await
            .unwrap();
        // Some rows imported (n/v rows), some skipped (ppr/ptc/etc).
        assert!(
            summary.imported > 0,
            "expected some imports, got {summary:?}"
        );
        assert!(
            !summary.skipped.is_empty(),
            "expected some skipped rows for unknown classes"
        );
        // No new word_classes should have been auto-created.
        let classes = WordClassRepository::new(state.clone())
            .list_all(lang.id)
            .await
            .unwrap();
        let abbrs: Vec<&str> = classes.iter().map(|c| c.abbreviation.as_str()).collect();
        assert!(
            !abbrs.contains(&"ppr") && !abbrs.contains(&"ptc"),
            "no unknown classes should have been auto-created with SkipRow policy, got {abbrs:?}"
        );
    }

    #[tokio::test]
    async fn test_has_header_skips_first_row() {
        let (state, user, lang) = setup().await;
        let mut opts = ImportOptions::defaults();
        opts.has_header = true;

        let summary = import_csv_bytes(&state, &user, &lang, SXS, &opts)
            .await
            .unwrap();
        // SXS has 3 data rows; with has_header=true, row 1 (feadhair) is treated as header.
        assert_eq!(
            summary.imported, 2,
            "expected 2 imports (3 data rows minus 1 header), got {summary:?}"
        );
    }

    #[tokio::test]
    async fn test_blank_notes_column_yields_empty_notes() {
        let (state, user, lang) = setup().await;
        let mut opts = ImportOptions::defaults();
        opts.col_notes = None;

        let _ = import_csv_bytes(&state, &user, &lang, SXS, &opts)
            .await
            .unwrap();

        let words_repo = WordRepository::new(state.clone());
        let words = words_repo
            .search(
                &lang.id,
                PaginatedRequest {
                    limit: 10,
                    offset: 0,
                },
                WordSearch::default(),
            )
            .await
            .unwrap();
        for w in &words.items {
            assert!(
                w.notes.is_empty(),
                "expected empty notes when col_notes is None, got {:?}",
                w.notes
            );
        }
    }

    #[tokio::test]
    async fn test_import_as_owner_creates_no_activities_or_audit_logs() {
        let (state, user, lang) = setup().await;

        let summary = import_csv_bytes(&state, &user, &lang, SXS, &ImportOptions::defaults())
            .await
            .unwrap();
        assert_eq!(summary.imported, 3);

        // No word- or definition-shaped activity rows from the import. (The
        // language creation in `setup` writes a single `create_language`
        // activity, which we ignore here.)
        let word_activities: i64 = sqlx::query_scalar!(
            "select count(*) from user_activities
             where user_id = $1 and entity_type in ('word', 'definition')",
            user.id
        )
        .fetch_one(&state.pool)
        .await
        .unwrap()
        .unwrap_or(0);
        assert_eq!(
            word_activities, 0,
            "import must not create word/definition activities, got {word_activities}"
        );

        // The owner has direct editor permission, so no audit log either.
        let audit_count: i64 = sqlx::query_scalar!(
            "select count(*) from audit_logs where user_id = $1",
            user.id
        )
        .fetch_one(&state.pool)
        .await
        .unwrap()
        .unwrap_or(0);
        assert_eq!(
            audit_count, 0,
            "owner-driven import should not create audit logs, got {audit_count}"
        );
    }

    #[tokio::test]
    async fn test_import_as_admin_creates_exactly_one_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Owner who owns the language
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let user_repo = UserRepository::new(app_state.clone());
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang = LanguageRepository::new(app_state.clone())
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "import test".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Admin user with no direct permission on the language
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        let summary =
            import_csv_bytes(&app_state, &admin, &lang, SXS, &ImportOptions::defaults())
                .await
                .unwrap();
        assert_eq!(summary.imported, 3);

        // No activities for admin either.
        let activity_count: i64 = sqlx::query_scalar!(
            "select count(*) from user_activities where user_id = $1",
            admin.id
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0);
        assert_eq!(
            activity_count, 0,
            "import must not create user_activities, got {activity_count}"
        );

        // Exactly one audit log entry, of action `imported` against the language.
        let logs = sqlx::query!(
            r#"
                select action as "action: crate::model::audit_log::AuditActionType",
                       resource_type as "resource_type: crate::model::audit_log::AuditableResource",
                       resource_id,
                       details
                from audit_logs
                where user_id = $1
            "#,
            admin.id
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            logs.len(),
            1,
            "expected exactly 1 audit log entry, got {}",
            logs.len()
        );
        let log = &logs[0];
        assert!(matches!(
            log.action,
            crate::model::audit_log::AuditActionType::Imported
        ));
        assert!(matches!(
            log.resource_type,
            crate::model::audit_log::AuditableResource::Language
        ));
        assert_eq!(log.resource_id, lang.id);
        assert_eq!(log.details["imported"], serde_json::json!(3));
        assert_eq!(log.details["skipped"], serde_json::json!(0));
    }
}
