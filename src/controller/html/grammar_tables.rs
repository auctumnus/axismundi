use askama::Template;
use axum::{
    Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{LanguagesWithContributors, okay, render_generic_error, render_template},
    err::AppError,
    get_user,
    md::render_md,
    model::{
        grammar_tables::{
            CreateGrammarTable, GrammarBody, GrammarCell, GrammarColumn, GrammarRow, GrammarTable,
            GrammarTableRepository, UpdateGrammarTable,
        },
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::LanguageRepository,
        sound_change_sets::SoundChangeSetRepository,
        word_categories::{WordCategory, WordCategoryRepository},
        word_classes::{WordClass, WordClassRepository},
        words::WordRepository,
    },
    util::extract_session::Session,
};

pub fn create_router() -> (Router<crate::util::AppState>, Router<crate::util::AppState>) {
    (
        Router::new()
            .route("/languages/{code}/grammar-tables/new", post(new_submit))
            .route("/languages/{code}/grammar-tables/swap", post(swap_submit))
            .route(
                "/languages/{code}/grammar-tables/{id}/edit-meta",
                post(edit_meta_submit),
            )
            .route(
                "/languages/{code}/grammar-tables/{id}/edit-body",
                post(edit_body_submit),
            ),
        Router::new()
            .route("/languages/{code}/grammar-tables", get(list))
            .route("/languages/{code}/grammar-tables/new", get(new_form))
            .route(
                "/languages/{code}/grammar-tables/{id}/edit-meta",
                get(edit_meta_form),
            )
            .route(
                "/languages/{code}/grammar-tables/{id}/edit-body",
                get(edit_body_form),
            )
            .route(
                "/languages/{code}/words/{slug}/{lemma}/grammar-tables/{id}",
                get(render_for_word),
            ),
    )
}

#[derive(Template)]
#[template(path = "languages/grammar-tables/list.html")]
#[allow(dead_code)]
struct GrammarTableListTemplate {
    current_user: Option<crate::model::users::User>,
    language: crate::model::languages::Language,
    rendered_tables: Vec<RenderedGrammarTable>,
    can_edit_language: bool,
    can_delete_language: bool,
}

struct RenderedGrammarTable {
    id: Uuid,
    html: String,
    previous_id: Option<Uuid>,
    next_id: Option<Uuid>,
}

async fn list(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    tables: GrammarTableRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table_list = attempt!(s, tables.list(&language).await);
    let ipa_estimator =
        attempt!(s, languages.get_ipa_estimator(language.id).await).map(|set| set.id);
    let can_edit_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };
    let can_delete_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };
    let rendered_html = futures::future::join_all(table_list.iter().map(|table| async {
        let edit_links = can_edit_language.then(|| {
            (
                format!(
                    "/languages/{}/grammar-tables/{}/edit-meta",
                    language.code, table.id
                ),
                format!(
                    "/languages/{}/grammar-tables/{}/edit-body",
                    language.code, table.id
                ),
            )
        });
        let edit_links = edit_links
            .as_ref()
            .map(|(metadata, body)| (metadata.as_str(), body.as_str()));
        match tables.random_example(table).await {
            Ok(Some(example)) => {
                let deadline = Instant::now()
                    + Duration::from_millis(crate::config::CONFIG.grammar.table_budget_ms);
                let rendered = crate::grammar::GrammarEvaluator::default()
                    .render(
                        &tables,
                        &sets,
                        ipa_estimator,
                        &example.word,
                        table,
                        deadline,
                    )
                    .await;
                match rendered {
                    Ok(rendered) => {
                        let table_html = crate::grammar::render_html_with_edit_links(
                            table, &rendered, edit_links,
                        )
                        .unwrap_or_else(|_| "<p>Failed to render table.</p>".to_owned());
                        format!(
                            "{table_html}{}",
                            crate::grammar::render_example_hint(
                                &language.code,
                                &example.word,
                                example.first_definition.as_deref(),
                            )
                        )
                    }
                    Err(_) => crate::grammar::render_definition_html(table, edit_links)
                        .unwrap_or_else(|_| "<p>Failed to render table.</p>".to_owned()),
                }
            }
            Ok(None) => crate::grammar::render_no_examples_html(table, edit_links),
            Err(_) => "<p>Failed to find an example for this table.</p>".to_owned(),
        }
    }))
    .await;
    let rendered_tables = rendered_html
        .into_iter()
        .enumerate()
        .map(|(index, html)| RenderedGrammarTable {
            id: table_list[index].id,
            html,
            previous_id: index.checked_sub(1).map(|index| table_list[index].id),
            next_id: table_list.get(index + 1).map(|table| table.id),
        })
        .collect();
    okay(render_template(GrammarTableListTemplate {
        current_user: s.user().cloned(),
        language,
        rendered_tables,
        can_edit_language,
        can_delete_language,
    }))
}

#[derive(Deserialize)]
struct SwapGrammarTablesForm {
    id1: Uuid,
    id2: Uuid,
}

async fn swap_submit(
    s: Session,
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path(code): Path<String>,
    axum::Form(form): axum::Form<SwapGrammarTablesForm>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    match tables.swap(&user, language.id, form.id1, form.id2).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{code}/grammar-tables")).into_response(),
        ),
        Err(error) => render_generic_error(s, error).await,
    }
}

#[derive(Template)]
#[template(path = "languages/grammar-tables/editor.html")]
#[allow(dead_code)]
struct GrammarTableEditorTemplate {
    current_user: Option<crate::model::users::User>,
    language: crate::model::languages::Language,
    error: Option<AppError>,
    editing: bool,
    action: String,
    previous_name: String,
    previous_description: String,
    previous_preamble: String,
    previous_table_body: GrammarBody,
    selected_word_class_ids: Vec<Uuid>,
    selected_category_ids: Vec<Uuid>,
    can_delete_language: bool,
    has_ipa_estimator: bool,
}

#[derive(Template)]
#[template(path = "languages/grammar-tables/metadata.html")]
#[allow(dead_code)]
struct GrammarTableMetadataTemplate {
    current_user: Option<crate::model::users::User>,
    language: crate::model::languages::Language,
    error: Option<AppError>,
    editing: bool,
    action: String,
    previous_name: String,
    previous_description: String,
    word_classes: Vec<WordClass>,
    word_categories: Vec<WordCategory>,
    word_class_options_json: String,
    word_category_options_json: String,
    selected_word_class_ids: Vec<Uuid>,
    selected_category_ids: Vec<Uuid>,
    can_delete_language: bool,
}

fn scope_options_json(options: impl Iterator<Item = (Uuid, String, String)>) -> String {
    let items: Vec<serde_json::Value> = options
        .map(|(id, name, abbreviation)| {
            serde_json::json!({ "id": id, "name": name, "abbreviation": abbreviation })
        })
        .collect();
    serde_json::to_string(&items)
        .unwrap_or_else(|_| "[]".to_owned())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

#[derive(Deserialize)]
struct GrammarTableForm {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    preamble: String,
    body: Option<String>,
    #[serde(
        default,
        rename = "word_class_ids[]",
        deserialize_with = "deserialize_uuid_vec"
    )]
    word_class_ids: Vec<Uuid>,
    #[serde(
        default,
        rename = "category_ids[]",
        deserialize_with = "deserialize_uuid_vec"
    )]
    category_ids: Vec<Uuid>,
}

/// HTML form parsers represent a field submitted once as a scalar, rather
/// than a one-item sequence. Accept both shapes for our multi-select fields.
fn deserialize_uuid_vec<'de, D>(deserializer: D) -> Result<Vec<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    struct UuidVecVisitor;

    impl<'de> Visitor<'de> for UuidVecVisitor {
        type Value = Vec<Uuid>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a UUID or a sequence of UUIDs")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Uuid::parse_str(value).map(|id| vec![id]).map_err(E::custom)
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut ids = Vec::new();
            while let Some(id) = sequence.next_element()? {
                ids.push(id);
            }
            Ok(ids)
        }
    }

    deserializer.deserialize_any(UuidVecVisitor)
}

async fn metadata_template(
    s: Session,
    languages: &LanguageRepository,
    word_classes: &WordClassRepository,
    word_categories: &WordCategoryRepository,
    permissions: &LanguagePermissionRepository,
    code: &str,
    editing: bool,
    action: String,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    selected_word_class_ids: Vec<Uuid>,
    selected_category_ids: Vec<Uuid>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(code).await);
    let classes = attempt!(s, word_classes.list_all(language.id).await);
    let categories = attempt!(s, word_categories.list_all(language.id).await);
    let word_class_options_json = scope_options_json(
        classes
            .iter()
            .map(|class| (class.id, class.name.clone(), class.abbreviation.clone())),
    );
    let word_category_options_json = scope_options_json(categories.iter().map(|category| {
        (
            category.id,
            category.name.clone(),
            category.abbreviation.clone(),
        )
    }));
    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);
    okay(render_template(GrammarTableMetadataTemplate {
        current_user: Some(user),
        language,
        error,
        editing,
        action,
        previous_name,
        previous_description,
        word_classes: classes,
        word_categories: categories,
        word_class_options_json,
        word_category_options_json,
        selected_word_class_ids,
        selected_category_ids,
        can_delete_language,
    }))
}

fn default_empty_body() -> GrammarBody {
    let row = |heading: &str| GrammarRow::Individual {
        heading: heading.into(),
        autogenerated: true,
        cells: vec![
            GrammarCell::default(),
            GrammarCell::default(),
            GrammarCell::default(),
        ],
    };
    GrammarBody {
        rows: vec![row("Row 1"), row("Row 2"), row("Row 3")],
        columns: vec![
            GrammarColumn::Individual {
                heading: "Column 1".into(),
                autogenerated: true,
            },
            GrammarColumn::Individual {
                heading: "Column 2".into(),
                autogenerated: true,
            },
            GrammarColumn::Individual {
                heading: "Column 3".into(),
                autogenerated: true,
            },
        ],
    }
}

async fn editor_template(
    s: Session,
    languages: &LanguageRepository,
    permissions: &LanguagePermissionRepository,
    code: &str,
    editing: bool,
    action: String,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    previous_preamble: String,
    previous_table_body: GrammarBody,
    selected_word_class_ids: Vec<Uuid>,
    selected_category_ids: Vec<Uuid>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(code).await);
    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);
    let has_ipa_estimator = attempt!(s, languages.get_ipa_estimator(language.id).await).is_some();
    okay(render_template(GrammarTableEditorTemplate {
        current_user: Some(user),
        language,
        error,
        editing,
        action,
        previous_name,
        previous_description,
        previous_preamble,
        previous_table_body,
        selected_word_class_ids,
        selected_category_ids,
        can_delete_language,
        has_ipa_estimator,
    }))
}

async fn new_form(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    metadata_template(
        s,
        &languages,
        &word_classes,
        &word_categories,
        &permissions,
        &code,
        false,
        format!("/languages/{code}/grammar-tables/new"),
        None,
        String::new(),
        String::new(),
        vec![],
        vec![],
    )
    .await
}

async fn edit_meta_form(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, tables.get(&language, id).await);
    metadata_template(
        s,
        &languages,
        &word_classes,
        &word_categories,
        &permissions,
        &code,
        true,
        format!("/languages/{code}/grammar-tables/{id}/edit-meta"),
        None,
        table.name,
        table.description,
        table.word_class_ids,
        table.category_ids,
    )
    .await
}

async fn edit_body_form(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, tables.get(&language, id).await);
    let body = attempt!(s, table.body());
    editor_template(
        s,
        &languages,
        &permissions,
        &code,
        true,
        format!("/languages/{code}/grammar-tables/{id}/edit-body"),
        None,
        table.name,
        table.description,
        table.preamble,
        body,
        table.word_class_ids,
        table.category_ids,
    )
    .await
}

async fn new_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    tables: GrammarTableRepository,
    Path(code): Path<String>,
    axum::Form(form): axum::Form<GrammarTableForm>,
) -> (StatusCode, Response) {
    if form.body.is_none() {
        if form.name.trim().is_empty() {
            return metadata_template(
                s,
                &languages,
                &word_classes,
                &word_categories,
                &permissions,
                &code,
                false,
                format!("/languages/{code}/grammar-tables/new"),
                Some(crate::err::bad_request("Name is required.")),
                form.name,
                form.description,
                form.word_class_ids,
                form.category_ids,
            )
            .await;
        }
        if form.word_class_ids.is_empty() {
            return metadata_template(
                s,
                &languages,
                &word_classes,
                &word_categories,
                &permissions,
                &code,
                false,
                format!("/languages/{code}/grammar-tables/new"),
                Some(crate::err::bad_request("Choose at least one word class.")),
                form.name,
                form.description,
                form.word_class_ids,
                form.category_ids,
            )
            .await;
        }
        return editor_template(
            s,
            &languages,
            &permissions,
            &code,
            false,
            format!("/languages/{code}/grammar-tables/new"),
            None,
            form.name,
            form.description,
            String::new(),
            default_empty_body(),
            form.word_class_ids,
            form.category_ids,
        )
        .await;
    }

    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let body = match serde_json::from_str::<GrammarBody>(form.body.as_deref().unwrap_or("{}")) {
        Ok(body) => body,
        Err(error) => {
            return editor_template(
                s,
                &languages,
                &permissions,
                &code,
                false,
                format!("/languages/{code}/grammar-tables/new"),
                Some(crate::err::bad_request(format!(
                    "Invalid table body: {error}"
                ))),
                form.name,
                form.description,
                form.preamble,
                default_empty_body(),
                form.word_class_ids,
                form.category_ids,
            )
            .await;
        }
    };
    let request = CreateGrammarTable {
        language_id: language.id,
        name: form.name.clone(),
        description: form.description.clone(),
        preamble: form.preamble.clone(),
        body: body.clone(),
        word_class_ids: form.word_class_ids.clone(),
        category_ids: form.category_ids.clone(),
    };
    match tables.create(&user, request).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{code}/grammar-tables")).into_response(),
        ),
        Err(error) => {
            editor_template(
                s,
                &languages,
                &permissions,
                &code,
                false,
                format!("/languages/{code}/grammar-tables/new"),
                Some(error),
                form.name,
                form.description,
                form.preamble,
                body,
                form.word_class_ids,
                form.category_ids,
            )
            .await
        }
    }
}

async fn edit_meta_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
    axum::Form(form): axum::Form<GrammarTableForm>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, tables.get(&language, id).await);
    let body = attempt!(s, table.body());
    let request = UpdateGrammarTable {
        name: form.name.clone(),
        description: form.description.clone(),
        preamble: table.preamble,
        body,
        word_class_ids: form.word_class_ids.clone(),
        category_ids: form.category_ids.clone(),
    };
    match tables.update(&user, id, request).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{code}/grammar-tables")).into_response(),
        ),
        Err(error) => {
            metadata_template(
                s,
                &languages,
                &word_classes,
                &word_categories,
                &permissions,
                &code,
                true,
                format!("/languages/{code}/grammar-tables/{id}/edit-meta"),
                Some(error),
                form.name,
                form.description,
                form.word_class_ids,
                form.category_ids,
            )
            .await
        }
    }
}

async fn edit_body_submit(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
    axum::Form(form): axum::Form<GrammarTableForm>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    // Keep the nested HTML mutation scoped exactly like the API mutation. The
    // form carries the table id, but the language in the URL is authoritative.
    attempt!(s, tables.get(&language, id).await);
    let body = match serde_json::from_str::<GrammarBody>(form.body.as_deref().unwrap_or("{}")) {
        Ok(body) => body,
        Err(error) => {
            return editor_template(
                s,
                &languages,
                &permissions,
                &code,
                true,
                format!("/languages/{code}/grammar-tables/{id}/edit-body"),
                Some(crate::err::bad_request(format!(
                    "Invalid table body: {error}"
                ))),
                form.name,
                form.description,
                form.preamble,
                default_empty_body(),
                form.word_class_ids,
                form.category_ids,
            )
            .await;
        }
    };
    let request = UpdateGrammarTable {
        name: form.name.clone(),
        description: form.description.clone(),
        preamble: form.preamble.clone(),
        body: body.clone(),
        word_class_ids: form.word_class_ids.clone(),
        category_ids: form.category_ids.clone(),
    };
    match tables.update(&user, id, request).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{code}/grammar-tables")).into_response(),
        ),
        Err(error) => {
            editor_template(
                s,
                &languages,
                &permissions,
                &code,
                true,
                format!("/languages/{code}/grammar-tables/{id}/edit-body"),
                Some(error),
                form.name,
                form.description,
                form.preamble,
                body,
                form.word_class_ids,
                form.category_ids,
            )
            .await
        }
    }
}

#[derive(Template)]
#[template(path = "languages/grammar-tables/render.html")]
struct RenderGrammarTableTemplate {
    current_user: Option<crate::model::users::User>,
    language: LanguagesWithContributors,
    table: GrammarTable,
    content: String,
    rendered_description: String,
    can_delete_language: bool,
}

async fn render_for_word(
    session: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    words: WordRepository,
    tables: GrammarTableRepository,
    permissions: LanguagePermissionRepository,
    Path((code, slug, lemma, id)): Path<(String, String, i32, Uuid)>,
) -> (StatusCode, Response) {
    let language = attempt!(session, languages.find_by_code(&code).await);
    let word = attempt!(
        session,
        words
            .find_by_slug_and_lemma(None, language.id, &slug, lemma)
            .await
    );
    let table = match tables.matching_table_for_word(&word, id).await {
        Ok(Some(table)) => table,
        Ok(None) => {
            return render_generic_error(
                session,
                crate::err::not_found("This grammar table does not apply to this word."),
            )
            .await;
        }
        Err(error) => return render_generic_error(session, error).await,
    };
    let deadline =
        Instant::now() + Duration::from_millis(crate::config::CONFIG.grammar.full_page_budget_ms);
    let ipa_estimator = match languages.get_ipa_estimator(language.id).await {
        Ok(estimator) => estimator.map(|set| set.id),
        Err(error) => return render_generic_error(session, error).await,
    };
    let can_edit_language = if let Some(user) = session.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };
    let edit_links = can_edit_language.then(|| {
        (
            format!("/languages/{code}/grammar-tables/{id}/edit-meta"),
            format!("/languages/{code}/grammar-tables/{id}/edit-body"),
        )
    });
    let rendered = crate::grammar::GrammarEvaluator::default()
        .render(&tables, &sets, ipa_estimator, &word, &table, deadline)
        .await;
    let content = match rendered {
        Ok(rendered) => match crate::grammar::render_html_with_edit_links(
            &table,
            &rendered,
            edit_links
                .as_ref()
                .map(|(metadata, body)| (metadata.as_str(), body.as_str())),
        ) {
            Ok(html) => html,
            Err(error) => format!("<p class=\"error\">{}</p>", askama::filters::escape(&error.to_string(), askama::filters::Html).unwrap()),
        },
        Err(crate::grammar::GrammarRenderError::TimedOut) => "<p>this took too long to render. try reducing the rule count or reducing the number of unique cells.</p>".into(),
        Err(crate::grammar::GrammarRenderError::Failed(error)) => format!("<p class=\"error\">{}</p>", askama::filters::escape(&error, askama::filters::Html).unwrap()),
    };
    let rendered_description = if table.description.is_empty() {
        String::new()
    } else {
        attempt!(session, render_md(&table.description).map_err(Into::into))
    };
    let can_delete_language = if let Some(user) = session.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };
    let language = attempt!(
        session,
        languages.materialize(language, session.user()).await
    );
    let body = render_template(RenderGrammarTableTemplate {
        current_user: session.user().cloned(),
        language,
        table,
        content,
        rendered_description,
        can_delete_language,
    });
    okay(body)
}
