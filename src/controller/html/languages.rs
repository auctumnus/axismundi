use askama::Template;
use axum::{Router, extract::Path, response::{IntoResponse, Redirect, Response}, routing::{get, post}, Form};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::{attempt, controller::html::{okay, render_template}, err::AppError, get_user, model::{contribution_stats::{ContributionsSearch, ContributionStatsRepository}, definitions::{Definition, DefinitionRepository}, language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, languages::{CreateLanguage, Language, LanguageRepository}, translatable::TranslatableRepository, translations::TranslationRepository, users::{User, UserRepository}, words::{Word, WordRepository, WordSearch}}, pagination::PaginatedRequest, util::{AppState, extract_session::Session}};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-language", post(new_language_submit))
        .route("/languages/{code}/edit", post(edit_language_submit))
        .route("/languages/{code}/permissions/{id}/delete", post(delete_permission_submit))
        .route("/languages/{code}/permissions/{id}/edit", post(edit_permission_submit));
    let normal_routes = Router::<AppState>::new()
        .route("/new-language", get(new_language_form))
        .route("/languages/{code}", get(view_language))
        .route("/languages/{code}/edit", get(edit_language_form))
        .route("/languages/{code}/contributors", get(search_contributors))
        .route("/languages/{code}/permissions/{id}/delete", get(delete_permission_form))
        .route("/languages/{code}/permissions/{id}/edit", get(edit_permission_form));

    (secure_routes, normal_routes)
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

async fn new_language_submit(s: Session, languages: LanguageRepository, form: axum::Form<NewLanguageFormData>) -> (StatusCode, Response) {
    let user = get_user!(s);

    match languages.create(&user, CreateLanguage {
        code: form.code.clone(),
        name: form.name.clone(),
        description: form.description.clone(),
        private: false,
    }).await {
        Ok(lang) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}", lang.code)).into_response()),
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

#[axum::debug_handler(state=AppState)]
async fn view_language(s: Session, languages: LanguageRepository, definitions: DefinitionRepository, users: UserRepository, words: WordRepository, translations: TranslationRepository, translatables: TranslatableRepository, permissions: LanguagePermissionRepository, invites: crate::model::language_invites::LanguageInviteRepository, axum::extract::Path(code): axum::extract::Path<String>) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let owner = attempt!(s, languages.find_owner(language.id).await);
    let contributor_count = attempt!(s, languages.count_contributors(language.id).await);
    let rendered_description = attempt!(s, languages.render_description(&language).await);
    let recent_words = attempt!(s, words.search(&language.id, PaginatedRequest {
        limit: 5,
        offset: 0,
    }, WordSearch {
        ..Default::default()
    }).await);

    let recent_translations = attempt!(s, translations.list_by_language(language.id, PaginatedRequest {
        limit: 5,
        offset: 0,
    }).await);

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
        languages.is_liked(&language.id, &user.id).await.unwrap_or(false)
    } else {
        false
    };

    // Fetch authors for each word
    let mut words_with_meta = Vec::new();
    for word in recent_words.items {
        let creator = attempt!(s, words.find_creator(&word.id).await);
        let first_definition = attempt!(s, definitions.get_first_by_word(&word.id).await);
        words_with_meta.push(WordWithMeta { word, first_definition, creator });
    }

    // Fetch authors and translatables for each translation
    let mut translations_with_authors = Vec::new();
    for translation in recent_translations.items {
        let author = attempt!(s, users.find_by_id(translation.created_by).await);
        let translatable = attempt!(s, translatables.find_by_id(translation.translatable).await);
        translations_with_authors.push(TranslationWithAuthor {
            translation,
            translatable,
            author
        });
    }

    // Check for pending invites
    let pending_invite = if let Some(user) = s.user() {
        match invites.find_by_language_and_recipient_unchecked(language.id, user.id).await {
            Ok(Some(invite)) if invite.accepted_at.is_none() => {
                // Fetch the sender
                match users.find_by_id(invite.sender).await {
                    Ok(sender) => Some((invite, sender)),
                    Err(_) => None,
                }
            },
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

    let body = render_template(template);
    okay(body)
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
}

async fn edit_language_form(s: Session, languages: LanguageRepository, permissions: LanguagePermissionRepository, axum::extract::Path(code): axum::extract::Path<String>) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let template = EditLanguageFormTemplate {
        current_user: Some(user),
        language: language.clone(),
        error: None,
        previous_code: language.code,
        previous_name: language.name,
        previous_description: language.description,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditLanguageFormData {
    code: String,
    name: String,
    description: String,
}

async fn edit_language_submit(s: Session, languages: LanguageRepository, permissions: LanguagePermissionRepository, axum::extract::Path(code): axum::extract::Path<String>, form: axum::Form<EditLanguageFormData>) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let updates = crate::model::languages::UpdateLanguage {
        code: if form.code != language.code { Some(form.code.clone()) } else { None },
        name: if form.name != language.name { Some(form.name.clone()) } else { None },
        description: if form.description != language.description { Some(form.description.clone()) } else { None },
        private: None,
    };

    match languages.update(&user, language.id, updates).await {
        Ok(lang) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}", lang.code)).into_response()),
        Err(e) => {
            let template = EditLanguageFormTemplate {
                can_delete_language: permissions
                    .has_permission(user.id, language.id, PermissionLevel::Owner)
                    .await
                    .unwrap_or(false),
                current_user: Some(user),
                language: language.clone(),
                error: Some(e),
                previous_code: form.code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                can_edit_language,
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
    previous_pagination: PaginatedRequest,
}


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

    let contributor_records = attempt!(s, contribution_stats.search_top_contributors(
        &language.id,
        &search,
        &pagination,
    ).await);

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
}

async fn delete_permission_form(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(s, crate::err::not_found("Permission not found")).await;
    }

    let target_user = attempt!(s, users.find_by_id(permission.user).await);

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
    );

    let template = DeletePermissionTemplate {
        current_user: Some(user),
        language,
        permission,
        target_user,
        user_has_permission,
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
        return crate::controller::html::render_generic_error(s, crate::err::not_found("Permission not found")).await;
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
}

async fn edit_permission_form(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(s, crate::err::not_found("Permission not found")).await;
    }

    let target_user = attempt!(s, users.find_by_id(permission.user).await);

    let can_grant_owner = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
    );

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
    );

    let template = EditPermissionTemplate {
        current_user: Some(user),
        language,
        permission,
        target_user,
        can_grant_owner,
        user_has_permission,
        error: None,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditPermissionFormData {
    permission: PermissionLevel,
}

async fn edit_permission_submit(
    s: Session,
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
        return crate::controller::html::render_generic_error(s, crate::err::not_found("Permission not found")).await;
    }

    let can_grant_owner = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
    );

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
    );

    match permissions.update_permission_checked(&user, id, form.permission).await {
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
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}
