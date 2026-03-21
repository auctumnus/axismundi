use askama::Template;
use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use markdown_it::plugins::extra::tables;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{LanguagesWithContributors, okay, render_generic_error, render_template},
    embed::{self, GenericEmbed, render_embed, truncate_description},
    err::AppError,
    get_user,
    model::{
        self, contribution_stats::ContributionStatsRepository, language_families::{
            FamilyWithContributors, LanguageFamilyRepository, SearchLanguageFamilies,
        }, language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, languages::{CreateLanguage, Language, LanguageRepository, LanguageSearch}, phonology_tables::{PhonologyTableRepository, SearchPhonologyTable, TableRenderOptions}, translatable::TranslatableRepository, translations::TranslationRepository, users::{User, UserRepository}, words::{WordRepository, WordSearch, WordWithMeta}
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-language", post(new_language_submit))
        .route("/languages/{code}/edit", post(edit_language_submit))
        .route("/languages/{code}/delete", post(delete_language_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/new-language", get(new_language_form))
        .route("/languages", get(search_languages))
        .route("/languages/{code}", get(view_language))
        .route("/languages/{code}/edit", get(edit_language_form))
        .route("/languages/{code}/delete", get(delete_language_form));

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

pub struct TranslationWithAuthor {
    translation: crate::model::translations::Translation,
    translatable: crate::model::translatable::Translatable,
    author: User,
}

#[derive(Template)]
#[template(path = "languages/view.html")]
#[allow(dead_code)]
struct ViewLanguageTemplate {
    current_user: Option<User>,
    recent_words: Vec<WordWithMeta>,
    recent_translations: Vec<TranslationWithAuthor>,
    language: Language,
    primary_family: Option<FamilyWithContributors>,
    other_families: Vec<FamilyWithContributors>,
    owner: User,
    contributor_count: i64,
    rendered_description: String,
    can_edit_language: bool,
    can_delete_language: bool,
    is_liked: bool,
    pending_invite: Option<(crate::model::language_invites::LanguageInvite, User)>,
    json_ld: String,
    phonology_tables: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
async fn view_language(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    languages: LanguageRepository,
    language_families: LanguageFamilyRepository,
    users: UserRepository,
    words: WordRepository,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
    permissions: LanguagePermissionRepository,
    invites: crate::model::language_invites::LanguageInviteRepository,
    phonology_tables: PhonologyTableRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let owner = attempt!(s, languages.find_owner(language.id).await);
    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        println!("hi discord!");
        return okay(
            render_embed(
                embed::EmbedTarget::Discord,
                GenericEmbed {
                    title: language.name,
                    description: format!(
                        "{}\n\n⭐️ {}",
                        truncate_description(&language.description),
                        language.like_count
                    ),
                    author: Some(owner.clone()),
                    color: owner.gender,
                    url: format!(
                        "{}/languages/{}",
                        &crate::CONFIG.public_url_base,
                        language.code
                    ),
                    image: None,
                },
            )
            .await
            .into_response(),
        );
    }
    let contributor_count = attempt!(s, languages.count_contributors(language.id).await);
    let rendered_description = attempt!(s, LanguageRepository::render_description(&language));
    let get_five = PaginatedRequest {
        limit: 5,
        offset: 0,
    };
    let recent_words = attempt!(
        s,
        words
            .search(&language.id, get_five.clone(), WordSearch::default())
            .await
    );

    let recent_translations = attempt!(
        s,
        translations.list_by_language(language.id, get_five,).await
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
        let word = attempt!(s, words.materialize(word, s.user()).await);
        words_with_meta.push(word);
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

    let primary_family = attempt!(s, language_families.find_primary_family(&language).await);

    let primary_family = if let Some(family) = &primary_family {
        Some(attempt!(
            s,
            language_families
                .materialize(family.clone(), s.user())
                .await
        ))
    } else {
        None
    };

    let other_families = attempt!(
        s,
        language_families
            .search(
                SearchLanguageFamilies {
                    has_language: Some(language.code.clone()),
                    q: None,
                    owner: None,
                },
                PaginatedRequest {
                    limit: if primary_family.is_some() { 4 } else { 5 },
                    offset: 0
                }
            )
            .await
    );

    let other_families = if let Some(primary) = &primary_family {
        other_families
            .items
            .into_iter()
            .filter(|f| f.id != primary.family.id)
            .collect()
    } else {
        other_families.items
    };

    let mut other_families_materialized = vec![];
    for family in other_families {
        let materialized = attempt!(s, language_families.materialize(family, s.user()).await);
        other_families_materialized.push(materialized);
    }

    let all_tables = attempt!(s, phonology_tables.search(&language, PaginatedRequest { limit: 100, offset: 0 }, SearchPhonologyTable { q: None, created_after: None, created_before: None }).await);
    let mut tables = Vec::new();
    for table in all_tables.items {

        let options = TableRenderOptions {
            standalone_link: Some(format!("/languages/{}/phonology-tables/{}", language.code, table.id)),
            edit_links: None,
        };
        tables.push(attempt!(s, table.to_html(&options)));
    }

    let json_ld = attempt!(
        s,
        serde_json::to_string(&attempt!(s, languages.as_json_ld(&language).await))
            .map_err(Into::into)
    );

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
        primary_family,
        json_ld,
        other_families: other_families_materialized,
        phonology_tables: tables,
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

    let can_edit_language = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let can_delete_language = is_admin_or_mod
        || permissions
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

    let can_edit_language = is_admin_or_mod
        || permissions
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
            let can_delete_language = is_admin_or_mod
                || permissions
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

    let can_delete_language = is_admin_or_mod
        || permissions
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
