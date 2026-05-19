use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{
        LanguagesWithContributors,
        languages::{Breadcrumb, Footer},
        okay, render_generic_error, render_template,
    },
    err::AppError,
    get_user,
    md::render_md,
    model::{
        contribution_stats::ContributionStatsRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        phonology_tables::{
            Body, Cell, Column, CreatePhonologyTable, PhonologyTable, PhonologyTableRepository,
            Row, SearchPhonologyTable, TableRenderOptions, UpdatePhonologyTable,
        },
        users::User,
    },
    pagination::PaginatedRequest,
    util::{
        AppState,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/phonology-tables/new",
            post(new_phonology_table_submit),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}/edit-meta",
            post(edit_meta_submit),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}/edit-body",
            post(edit_body_submit),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}/delete",
            post(delete_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/phonology-tables",
            get(search_phonology_tables),
        )
        .route(
            "/languages/{code}/phonology-tables/new",
            get(new_phonology_table_form),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}",
            get(view_phonology_table),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}/edit-meta",
            get(edit_meta_form),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}/edit-body",
            get(edit_body_form),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}/delete",
            get(delete_form),
        );

    (secure_routes, normal_routes)
}

// --- search ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/fragments/list_header.html")]
struct PtSearchHeader {
    can_edit_language: bool,
    language_code: String,
    language: LanguagesWithContributors,
}

#[derive(Template)]
#[template(path = "languages/phonology-tables/fragments/query.html")]
struct PtSearchQueryTemplate {
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    created_before: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Template)]
#[template(path = "languages/phonology-tables/fragments/card.html")]
#[allow(dead_code)]
struct PtCard {
    rendered_html: String,
}

async fn search_phonology_tables(
    s: Session,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path(code): Path<String>,
    axum::extract::Query(query): axum::extract::Query<SearchPhonologyTable>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&code).await);

    let search_action = format!("/languages/{}/phonology-tables", code);

    let top_contributors = attempt!(
        s,
        contribution_stats
            .get_top_contributors(&language.id, 5)
            .await
    );
    let is_liked = if let Some(user) = &current_user {
        languages
            .is_liked(&user.id, &language.id)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let can_edit_language = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let can_delete_language = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let language_with_contributors = LanguagesWithContributors {
        language,
        top_contributors,
        is_liked,
    };

    let lang_code = language_with_contributors.language.code.clone();
    let results = phonology_tables
        .search(
            &language_with_contributors.language,
            pagination.clone(),
            query.clone(),
        )
        .await;

    let render_item = move |table: &PhonologyTable| {
        let options = TableRenderOptions {
            standalone_link: Some(format!(
                "/languages/{}/phonology-tables/{}",
                lang_code, table.id
            )),
            edit_links: None,
            header_el: "h3".to_string(),
        };
        let rendered_html = match table.to_html(&options) {
            Ok(html) => html,
            Err(_) => String::from("<p>Failed to render table.</p>"),
        };
        PtCard { rendered_html }
    };

    let header = PtSearchHeader {
        can_edit_language,
        language_code: language_with_contributors.language.code.clone(),
        language: language_with_contributors.clone(),
    };

    let query_template = PtSearchQueryTemplate {
        created_after: query.created_after,
        created_before: query.created_before,
    };

    let breadcrumbs = Breadcrumb {
        language: &language_with_contributors.language,
    };

    let footer = Footer {
        language: &language_with_contributors.language,
        can_edit_language: can_delete_language,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template,
        query,
        results,
        pagination,
        search_name: "phonology tables",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();
    (status, render_template(template))
}

// --- new step 1 ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/new-1.html")]
#[allow(dead_code)]
struct NewPhonologyTableStep1Template {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    will_create_audit_log: bool,
    can_delete_language: bool,
}

// --- new step 2 ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/new-2.html")]
#[allow(dead_code)]
struct NewPhonologyTableStep2Template {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    previous_table_body: Body,
    will_create_audit_log: bool,
    can_delete_language: bool,
}

// --- view ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/view.html")]
#[allow(dead_code)]
struct ViewPhonologyTableTemplate {
    current_user: Option<User>,
    language: LanguagesWithContributors,
    table: PhonologyTable,
    rendered_html: String,
    rendered_description: String,
    can_edit_language: bool,
    can_delete_language: bool,
}

// --- edit meta ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/edit-meta.html")]
#[allow(dead_code)]
struct EditMetaTemplate {
    current_user: Option<User>,
    language: Language,
    table: PhonologyTable,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    will_create_audit_log: bool,
    can_delete_language: bool,
}

// --- edit body ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/edit-body.html")]
#[allow(dead_code)]
struct EditBodyTemplate {
    current_user: Option<User>,
    language: Language,
    table: PhonologyTable,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    previous_table_body: Body,
    will_create_audit_log: bool,
    can_delete_language: bool,
}

// --- delete ---

#[derive(Template)]
#[template(path = "languages/phonology-tables/delete.html")]
#[allow(dead_code)]
struct DeletePhonologyTableTemplate {
    current_user: Option<User>,
    language: Language,
    table: PhonologyTable,
    will_create_audit_log: bool,
    can_delete_language: bool,
}

// --- form data ---

#[derive(Deserialize)]
struct NewPhonologyTableFormData {
    name: String,
    description: String,
    step: String,
    body: Option<String>,
}

#[derive(Deserialize)]
struct EditMetaFormData {
    name: String,
    description: String,
}

#[derive(Deserialize)]
struct EditBodyFormData {
    body: String,
}

// --- handlers ---

fn default_empty_body() -> Body {
    Body {
        rows: vec![
            Row::Individual {
                heading: "Row 1".to_string(),
                autogenerated: true,
                cells: vec![
                    Cell { phonemes: vec![] },
                    Cell { phonemes: vec![] },
                    Cell { phonemes: vec![] },
                ],
            },
            Row::Individual {
                heading: "Row 2".to_string(),
                autogenerated: true,
                cells: vec![
                    Cell { phonemes: vec![] },
                    Cell { phonemes: vec![] },
                    Cell { phonemes: vec![] },
                ],
            },
            Row::Individual {
                heading: "Row 3".to_string(),
                autogenerated: true,
                cells: vec![
                    Cell { phonemes: vec![] },
                    Cell { phonemes: vec![] },
                    Cell { phonemes: vec![] },
                ],
            },
        ],
        columns: vec![
            Column::Individual {
                heading: "Column 1".to_string(),
                autogenerated: true,
            },
            Column::Individual {
                heading: "Column 2".to_string(),
                autogenerated: true,
            },
            Column::Individual {
                heading: "Column 3".to_string(),
                autogenerated: true,
            },
        ],
        annotations: vec![],
    }
}

async fn new_phonology_table_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = NewPhonologyTableStep1Template {
        current_user: Some(user),
        language,
        error: None,
        previous_name: String::new(),
        previous_description: String::new(),
        will_create_audit_log,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn new_phonology_table_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    form: axum::Form<NewPhonologyTableFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    if form.step == "1" {
        // Validate name is non-empty, then show step 2
        if form.name.trim().is_empty() {
            let template = NewPhonologyTableStep1Template {
                current_user: Some(user),
                language,
                error: Some(crate::err::bad_request("Name is required.")),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                will_create_audit_log,
                can_delete_language,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }

        let template = NewPhonologyTableStep2Template {
            current_user: Some(user),
            language,
            error: None,
            previous_name: form.name.clone(),
            previous_description: form.description.clone(),
            previous_table_body: default_empty_body(),
            will_create_audit_log,
            can_delete_language,
        };

        return okay(render_template(template));
    }

    // Step 2: parse body and create
    let body_str = form.body.as_deref().unwrap_or("{}");
    let body: Body = match serde_json::from_str(body_str) {
        Ok(b) => b,
        Err(e) => {
            let template = NewPhonologyTableStep2Template {
                current_user: Some(user),
                language,
                error: Some(crate::err::bad_request(format!(
                    "Invalid table body: {}",
                    e
                ))),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                previous_table_body: default_empty_body(),
                will_create_audit_log,
                can_delete_language,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    match phonology_tables
        .create(
            &user,
            CreatePhonologyTable {
                language_id: language.id,
                name: form.name.clone(),
                description,
                body: body.clone(),
            },
        )
        .await
    {
        Ok(table) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/phonology-tables/{}",
                code, table.id
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = NewPhonologyTableStep2Template {
                current_user: Some(user),
                language,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                previous_table_body: body,
                will_create_audit_log,
                can_delete_language,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

async fn view_phonology_table(
    s: Session,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    let top_contributors = attempt!(
        s,
        contribution_stats
            .get_top_contributors(&language.id, 5)
            .await
    );
    let is_liked = if let Some(user) = s.user() {
        attempt!(s, languages.is_liked(&language.id, &user.id).await)
    } else {
        false
    };

    let can_edit_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let edit_links = if can_edit_language {
        Some((
            format!(
                "/languages/{}/phonology-tables/{}/edit-meta",
                language.code, table.id
            ),
            format!(
                "/languages/{}/phonology-tables/{}/edit-body",
                language.code, table.id
            ),
            format!(
                "/languages/{}/phonology-tables/{}/delete",
                language.code, table.id
            ),
        ))
    } else {
        None
    };
    let options = TableRenderOptions {
        standalone_link: None,
        edit_links: edit_links,
        header_el: "h2".to_string(),
    };

    let rendered_html = attempt!(s, table.to_html(&options));

    let can_delete_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let language = LanguagesWithContributors {
        language,
        top_contributors,
        is_liked,
    };

    let rendered_description = if !table.description.is_empty() {
        attempt!(s, render_md(&table.description).map_err(Into::into))
    } else {
        String::new()
    };

    let template = ViewPhonologyTableTemplate {
        current_user: s.user().cloned(),
        language,
        table,
        rendered_html,
        rendered_description,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn edit_meta_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = EditMetaTemplate {
        current_user: Some(user),
        language,
        previous_name: table.name.clone(),
        previous_description: table.description.clone(),
        table,
        error: None,
        will_create_audit_log,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn edit_meta_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
    form: axum::Form<EditMetaFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    let updates = UpdatePhonologyTable {
        name: if form.name == table.name {
            None
        } else {
            Some(form.name.clone())
        },
        description: if description.as_deref().unwrap_or("") == table.description {
            None
        } else {
            description
        },
        body: None,
    };

    match phonology_tables.update(&user, table.id, updates).await {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/phonology-tables/{}",
                code, updated.id
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = EditMetaTemplate {
                current_user: Some(user),
                language,
                table,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                will_create_audit_log,
                can_delete_language,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

async fn edit_body_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let previous_table_body: Body = attempt!(
        s,
        serde_json::from_value(table.body.clone()).map_err(Into::into)
    );

    let template = EditBodyTemplate {
        current_user: Some(user),
        language,
        previous_name: table.name.clone(),
        previous_description: table.description.clone(),
        previous_table_body,
        table,
        error: None,
        will_create_audit_log,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn edit_body_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
    form: axum::Form<EditBodyFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let body: Body = match serde_json::from_str(&form.body) {
        Ok(b) => b,
        Err(e) => {
            let previous_table_body: Body =
                serde_json::from_value(table.body.clone()).unwrap_or_else(|_| default_empty_body());

            let template = EditBodyTemplate {
                current_user: Some(user),
                language,
                table,
                error: Some(crate::err::bad_request(format!(
                    "Invalid table body: {}",
                    e
                ))),
                previous_name: String::new(),
                previous_description: String::new(),
                previous_table_body,
                will_create_audit_log,
                can_delete_language,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let updates = UpdatePhonologyTable {
        name: None,
        description: None,
        body: Some(body.clone()),
    };

    match phonology_tables.update(&user, table.id, updates).await {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/phonology-tables/{}",
                code, updated.id
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = EditBodyTemplate {
                current_user: Some(user),
                language,
                table,
                error: Some(e),
                previous_name: String::new(),
                previous_description: String::new(),
                previous_table_body: body,
                will_create_audit_log,
                can_delete_language,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

async fn delete_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeletePhonologyTableTemplate {
        current_user: Some(user),
        language,
        table,
        will_create_audit_log,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn delete_submit(
    s: Session,
    languages: LanguageRepository,
    phonology_tables: PhonologyTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let table = attempt!(s, phonology_tables.get(&language, id).await);

    match phonology_tables.delete(&user, table.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
