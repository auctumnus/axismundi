use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{LanguagesWithContributors, okay, render_generic_error, render_template},
    err::AppError,
    get_user,
    model::{
        contribution_stats::{ContributionStatsRepository, ContributionsSearch},
        definitions::{Definition, DefinitionRepository},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{CreateLanguage, Language, LanguageRepository, LanguageSearch},
        translatable::TranslatableRepository,
        translations::TranslationRepository,
        users::{User, UserRepository},
        words::{Word, WordRepository, WordSearch},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-language", post(new_language_submit))
        .route("/languages/{code}/edit", post(edit_language_submit))
        .route("/languages/{code}/delete", post(delete_language_submit))
        .route(
            "/languages/{code}/permissions/{id}/delete",
            post(delete_permission_submit),
        )
        .route(
            "/languages/{code}/permissions/{id}/edit",
            post(edit_permission_submit),
        );
    let normal_routes = Router::<AppState>::new()
        .route("/new-language", get(new_language_form))
        .route("/languages", get(search_languages))
        .route("/languages/{code}", get(view_language))
        .route("/languages/{code}/edit", get(edit_language_form))
        .route("/languages/{code}/delete", get(delete_language_form))
        .route("/languages/{code}/contributors", get(search_contributors))
        .route(
            "/languages/{code}/permissions/{id}/delete",
            get(delete_permission_form),
        )
        .route(
            "/languages/{code}/permissions/{id}/edit",
            get(edit_permission_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "languages/search.html")]
struct SearchLanguagesTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_query: LanguageSearch,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<LanguagesWithContributors>>,
}

async fn search_languages(
    s: Session,
    languages: LanguageRepository,
    contribution_stats: ContributionStatsRepository,
    Query(query): Query<LanguageSearch>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    let query = LanguageSearch {
        text_query: query.text_query.and_then(|q| {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        owned_by: query.owned_by.and_then(|o| {
            let trimmed = o.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        ..query
    };

    let results = match languages.search(pagination.clone(), query.clone()).await {
        Ok(res) => res,
        Err(e) => {
            let template = SearchLanguagesTemplate {
                current_user,
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
            };
            let body = render_template(template);
            return (StatusCode::BAD_REQUEST, body);
        }
    };

    let mut results_with_meta = vec![];
    for language in results.items {
        let top_contributors = attempt!(
            s,
            contribution_stats
                .get_top_contributors(&language.id, 5)
                .await
        );
        let is_liked = if let Some(user) = &current_user {
            languages
                .is_liked(&language.id, &user.id)
                .await
                .unwrap_or(false)
        } else {
            false
        };
        results_with_meta.push(LanguagesWithContributors {
            language,
            top_contributors,
            is_liked,
        });
    }

    let results_with_meta = Some(PaginatedResponse {
        items: results_with_meta,
        total: results.total,
        limit: results.limit,
        offset: results.offset,
        has_more: results.has_more,
    });

    let template = SearchLanguagesTemplate {
        current_user,
        error: None,
        previous_query: query,
        previous_pagination: pagination,
        results: results_with_meta,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "languages/new.html")]
#[allow(dead_code)]
struct NewLanguageFormTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
}

async fn new_language_form(s: Session) -> (StatusCode, Response) {
    let user = get_user!(s);

    let template = NewLanguageFormTemplate {
        current_user: Some(user),
        error: None,
        previous_code: String::new(),
        previous_name: String::new(),
        previous_description: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewLanguageFormData {
    code: String,
    name: String,
    description: String,
}

async fn new_language_submit(
    s: Session,
    languages: LanguageRepository,
    form: axum::Form<NewLanguageFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    match languages
        .create(
            &user,
            CreateLanguage {
                code: form.code.clone(),
                name: form.name.clone(),
                description: form.description.clone(),
                private: false,
            },
        )
        .await
    {
        Ok(lang) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}", lang.code)).into_response(),
        ),
        Err(e) => {
            let template = NewLanguageFormTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_code: form.code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

struct WordWithMeta {
    word: Word,
    first_definition: Option<Definition>,
    creator: User,
}

struct TranslationWithAuthor {
    translation: crate::model::translations::Translation,
    translatable: crate::model::translatable::Translatable,
    author: User,
}

#[derive(Template)]
#[template(path = "languages/view.html")]
struct ViewLanguageTemplate {
    current_user: Option<User>,
    recent_words: Vec<WordWithMeta>,
    recent_translations: Vec<TranslationWithAuthor>,
    language: Language,
    owner: User,
    contributor_count: i64,
    rendered_description: String,
    can_edit_language: bool,
    can_delete_language: bool,
    is_liked: bool,
    pending_invite: Option<(crate::model::language_invites::LanguageInvite, User)>,
}

#[allow(clippy::too_many_arguments)]
async fn view_language(
    s: Session,
    languages: LanguageRepository,
    definitions: DefinitionRepository,
    users: UserRepository,
    words: WordRepository,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
    permissions: LanguagePermissionRepository,
    invites: crate::model::language_invites::LanguageInviteRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let owner = attempt!(s, languages.find_owner(language.id).await);
    let contributor_count = attempt!(s, languages.count_contributors(language.id).await);
    let rendered_description = attempt!(s, LanguageRepository::render_description(&language));
    let get_five = PaginatedRequest { limit: 5, offset: 0 };
    let recent_words = attempt!(
        s,
        words
            .search(
                &language.id,
                get_five.clone(),
                WordSearch::default()
            )
            .await
    );

    let recent_translations = attempt!(
        s,
        translations
            .list_by_language(
                language.id,
                get_five,
            )
            .await
    );

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

    let is_liked = if let Some(user) = s.user() {
        languages
            .is_liked(&language.id, &user.id)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    // Fetch authors for each word
    let mut words_with_meta = Vec::new();
    for word in recent_words.items {
        let creator = attempt!(s, words.find_creator(&word.id).await);
        let first_definition = attempt!(s, definitions.get_first_by_word(&word.id).await);
        words_with_meta.push(WordWithMeta {
            word,
            first_definition,
            creator,
        });
    }

    // Fetch authors and translatables for each translation
    let mut translations_with_authors = Vec::new();
    for translation in recent_translations.items {
        let author = attempt!(s, users.find_by_id(translation.created_by).await);
        let translatable = attempt!(s, translatables.find_by_id(translation.translatable).await);
        translations_with_authors.push(TranslationWithAuthor {
            translation,
            translatable,
            author,
        });
    }

    // Check for pending invites
    let pending_invite = if let Some(user) = s.user() {
        match invites
            .find_by_language_and_recipient_unchecked(language.id, user.id)
            .await
        {
            Ok(Some(invite)) if invite.accepted_at.is_none() => {
                match users.find_by_id(invite.sender).await {
                    Ok(sender) => Some((invite, sender)),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let template = ViewLanguageTemplate {
        current_user: s.user().cloned(),
        recent_words: words_with_meta,
        recent_translations: translations_with_authors,
        language,
        owner,
        contributor_count,
        rendered_description,
        can_edit_language,
        can_delete_language,
        is_liked,
        pending_invite,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "languages/edit.html")]
#[allow(dead_code)]
struct EditLanguageFormTemplate {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
}

async fn edit_language_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_language = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = EditLanguageFormTemplate {
        current_user: Some(user),
        language: language.clone(),
        error: None,
        previous_code: language.code,
        previous_name: language.name,
        previous_description: language.description,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditLanguageFormData {
    code: String,
    name: String,
    description: String,
}

async fn edit_language_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
    form: axum::Form<EditLanguageFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_language = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let updates = crate::model::languages::UpdateLanguage {
        code: if form.code == language.code {
            None
        } else {
            Some(form.code.clone())
        },
        name: if form.name == language.name {
            None
        } else {
            Some(form.name.clone())
        },
        description: if form.description == language.description {
            None
        } else {
            Some(form.description.clone())
        },
        private: None,
    };

    match languages.update(&user, language.id, updates).await {
        Ok(lang) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}", lang.code)).into_response(),
        ),
        Err(e) => {
            let can_delete_language = is_admin_or_mod || permissions
                .has_permission(user.id, language.id, PermissionLevel::Owner)
                .await
                .unwrap_or(false);

            let template = EditLanguageFormTemplate {
                can_delete_language,
                current_user: Some(user),
                language: language.clone(),
                error: Some(e),
                previous_code: form.code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                can_edit_language,
                will_create_audit_log,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

struct ContributorWithStats {
    user: User,
    permission: PermissionLevel,
    permission_id: Option<Uuid>,
    word_count: i64,
    translation_count: i64,
    can_edit: bool,
    can_delete: bool,
}

#[derive(Template)]
#[template(path = "languages/contributors.html")]
struct SearchContributorsTemplate {
    current_user: Option<User>,
    language: Language,
    contributors: Vec<ContributorWithStats>,
    user_has_permission: bool,
    previous_query: ContributionsSearch,
    #[allow(dead_code)]
    previous_pagination: PaginatedRequest,
}

#[allow(clippy::match_same_arms)] // easier to read like this
async fn search_contributors(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path(code): Path<String>,
    axum::extract::Query(search): axum::extract::Query<ContributionsSearch>,
    axum::extract::Query(pagination): axum::extract::Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);

    // Get current user's permission level
    let current_user_permission = if let Some(user) = s.user() {
        permissions
            .find_by_user_and_language(user.id, language.id)
            .await
            .ok()
            .flatten()
            .map(|p| p.permission)
    } else {
        None
    };

    let user_has_permission = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let contributor_records = attempt!(
        s,
        contribution_stats
            .search_top_contributors(&language.id, &search, &pagination,)
            .await
    );

    let current_user_id = s.user().map(|u| u.id);
    let mut contributors = Vec::new();
    for record in contributor_records.items {
        let user = record.0;
        let target_permission = record.2;
        let permission_id = record.3;

        // Determine if current user can edit/delete this contributor's permission
        let (can_edit, can_delete) = if let Some(current_perm) = current_user_permission {
            // Check if trying to modify own permission
            let is_self = current_user_id == Some(user.id);

            // Owner cannot delete/edit their own permission
            // Users can delete their own permission (except Owner)
            if is_self {
                let can_delete_self = current_perm != PermissionLevel::Owner;
                (false, can_delete_self)
            } else {
                // Check permission table from language_permissions.rs
                let can_modify = match (current_perm, target_permission) {
                    (PermissionLevel::Owner, PermissionLevel::Owner) => false,
                    (PermissionLevel::Owner, _) => true,
                    (PermissionLevel::Admin, PermissionLevel::Editor) => true,
                    (PermissionLevel::Admin, PermissionLevel::Viewer) => true,
                    _ => false,
                };
                (can_modify, can_modify)
            }
        } else {
            (false, false)
        };

        contributors.push(ContributorWithStats {
            user,
            permission: target_permission,
            permission_id,
            word_count: record.1.word_count,
            translation_count: record.1.translation_count,
            can_edit: can_edit && permission_id.is_some(),
            can_delete: can_delete && permission_id.is_some(),
        });
    }

    let template = SearchContributorsTemplate {
        current_user: s.user().cloned(),
        language,
        contributors,
        user_has_permission,
        previous_query: search,
        previous_pagination: pagination,
    };

    let body = render_template(template);
    okay(body)
}

// Delete permission handlers

#[derive(Template)]
#[template(path = "languages/delete_permission.html")]
struct DeletePermissionTemplate {
    current_user: Option<User>,
    language: Language,
    permission: crate::model::language_permissions::LanguagePermission,
    target_user: User,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn delete_permission_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    let target_user = attempt!(s, users.find_by_id(permission.user).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod || attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
    );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeletePermissionTemplate {
        current_user: Some(user),
        language,
        permission,
        target_user,
        user_has_permission,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_permission_submit(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    attempt!(s, permissions.delete_checked(&user, id).await);

    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/languages/{}/contributors", code)).into_response(),
    )
}

// Edit permission handlers

#[derive(Template)]
#[template(path = "languages/edit_permission.html")]
struct EditPermissionTemplate {
    current_user: Option<User>,
    language: Language,
    permission: crate::model::language_permissions::LanguagePermission,
    target_user: User,
    can_grant_owner: bool,
    user_has_permission: bool,
    error: Option<AppError>,
    will_create_audit_log: bool,
}

async fn edit_permission_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    let target_user = attempt!(s, users.find_by_id(permission.user).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_grant_owner = is_admin_or_mod || attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
    );

    let user_has_permission = is_admin_or_mod || attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
    );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = EditPermissionTemplate {
        current_user: Some(user),
        language,
        permission,
        target_user,
        can_grant_owner,
        user_has_permission,
        error: None,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditPermissionFormData {
    permission: PermissionLevel,
}

async fn edit_permission_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
    Form(form): Form<EditPermissionFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_grant_owner = is_admin_or_mod || attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
    );

    let user_has_permission = is_admin_or_mod || attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
    );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    match permissions
        .update_permission_checked(&user, id, form.permission)
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/contributors", code)).into_response(),
        ),
        Err(e) => {
            let target_user = attempt!(s, users.find_by_id(permission.user).await);
            let template = EditPermissionTemplate {
                current_user: Some(user),
                language,
                permission,
                target_user,
                can_grant_owner,
                user_has_permission,
                error: Some(e),
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

// Delete language handlers

#[derive(Template)]
#[template(path = "languages/delete.html")]
#[allow(dead_code)]
struct DeleteLanguageTemplate {
    current_user: Option<User>,
    language: Language,
    can_delete_language: bool,
    will_create_audit_log: bool,
}

async fn delete_language_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_delete_language = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeleteLanguageTemplate {
        current_user: Some(user),
        language,
        can_delete_language,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_language_submit(
    s: Session,
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    match languages.delete(&user, language.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/languages").into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
