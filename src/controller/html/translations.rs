use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{LanguagesWithContributors, TranslatableWithMeta, okay, render_template},
    embed::{EmbedTarget, GenericEmbed, render_embed, truncate_description},
    err::{AppError, bad_request},
    get_user,
    model::{
        contribution_stats::ContributionStatsRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        translatable::{Translatable, TranslatableRepository},
        translations::{
            CreateTranslation, Translation, TranslationRepository, TranslationSearch,
            UpdateTranslation,
        },
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, BackQuery, extract_session::Session, search_template::{SearchTemplateArgs, make_search_layout}},
};
use axum::extract::State;

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/translatable/{slug}/new-translation-1",
            post(new_translation_step_1_submit),
        )
        .route(
            "/translatable/{slug}/new-translation-2",
            post(new_translation_step_2_submit),
        )
        .route(
            "/translatable/{slug}/edit-translation",
            post(edit_translation_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{code}/translations", get(translation_search))
        .route(
            "/translatable/{slug}/translations",
            get(translation_search_by_translatable),
        )
        .route(
            "/translatable/{slug}/new-translation",
            get(new_translation_step_1_form),
        )
        .route(
            "/translatable/{slug}/translation/{code}",
            get(view_translation),
        )
        .route(
            "/translatable/{slug}/edit-translation/{code}",
            get(edit_translation_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Debug, Serialize)]
pub struct TranslationWithMeta {
    pub translation: Translation,
    pub author: User,
    pub is_liked: bool,
}

#[derive(Template)]
#[template(path = "translations/fragments/card.html")]
struct TranslationPreviewCard {
    translation_with_meta: TranslationWithMeta,
    back_url: String,
    kind: String,
}

#[derive(Template)]
#[template(path = "translations/fragments/list_header.html")]
#[allow(dead_code)]
struct TranslationSearchHeader {
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "translatables/fragments/breadcrumb.html")]
struct TranslatableBreadcrumb<'a> {
    translatable: &'a Translatable,
}

#[derive(Template)]
#[template(path = "translatables/fragments/footer.html")]
struct TranslatableFooter<'a> {
    translatable: &'a Translatable,
    can_edit_translatable: bool,
}

#[allow(clippy::too_many_arguments)]
async fn translation_search(
    s: Session,
    languages: LanguageRepository,
    translations: TranslationRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    Query(query): Query<TranslationSearch>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let search_action = format!("/languages/{}/translations", language.code);
    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let results = match translations
        .search(&language.id, pagination.clone(), query.clone())
        .await
    {
        Ok(response) => {
            let mut items = Vec::with_capacity(response.items.len());
            for translation in response.items {
                let author = attempt!(s, users.find_by_id(translation.created_by).await);
                let is_liked = if let Some(user) = &current_user {
                    translations.is_liked(&translation.id, &user.id).await.unwrap_or(false)
                } else {
                    false
                };
                items.push(TranslationWithMeta {
                    translation,
                    author,
                    is_liked,
                });
            }
            Ok(crate::pagination::PaginatedResponse {
                items,
                total: response.total,
                limit: response.limit,
                offset: response.offset,
                has_more: response.has_more,
            })
        },
        Err(e) => Err(e),
    };

    let render_item = move |item: &TranslationWithMeta| TranslationPreviewCard {
        translation_with_meta: TranslationWithMeta {
            translation: item.translation.clone(),
            author: item.author.clone(),
            is_liked: item.is_liked,
        },
        back_url: back_url.clone(),
        kind: "translatable".to_string(),
    };

    let header = TranslationSearchHeader {
        user_has_permission,
    };

    let breadcrumbs = crate::controller::html::languages::Breadcrumb {
        language: &language,
    };

    let footer = crate::controller::html::languages::Footer {
        language: &language,
        can_edit_language: user_has_permission,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template: crate::util::EmptyTemplate,
        query,
        results,
        pagination,
        search_name: "translations",
        search_action,
        render_item,
    }).with_breadcrumbs(breadcrumbs).with_footer(footer);

    let status = template.status();
    (status, render_template(template))
}

#[allow(clippy::too_many_arguments)]
async fn translation_search_by_translatable(
    s: Session,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    users: UserRepository,
    Path(slug): Path<String>,
    Query(query): Query<TranslationSearch>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let can_edit_translatable = if let Some(user) = &current_user {
        translatable.created_by == user.id
    } else {
        false
    };

    let search_action = format!("/translatable/{}/translations", translatable.slug);
    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let results = match translations
        .search_by_translatable(&translatable.id, pagination.clone(), query.clone())
        .await
    {
        Ok(response) => {
            let mut items = Vec::with_capacity(response.items.len());
            for translation in response.items {
                let author = attempt!(s, users.find_by_id(translation.created_by).await);
                let is_liked = if let Some(user) = &current_user {
                    translations.is_liked(&translation.id, &user.id).await.unwrap_or(false)
                } else {
                    false
                };
                items.push(TranslationWithMeta {
                    translation,
                    author,
                    is_liked,
                });
            }
            Ok(crate::pagination::PaginatedResponse {
                items,
                total: response.total,
                limit: response.limit,
                offset: response.offset,
                has_more: response.has_more,
            })
        },
        Err(e) => Err(e),
    };

    let render_item = move |item: &TranslationWithMeta| TranslationPreviewCard {
        translation_with_meta: TranslationWithMeta {
            translation: item.translation.clone(),
            author: item.author.clone(),
            is_liked: item.is_liked,
        },
        back_url: back_url.clone(),
        kind: "language".to_string(),
    };

    let header = TranslationSearchHeader {
        user_has_permission: false,
    };

    let breadcrumbs = TranslatableBreadcrumb {
        translatable: &translatable,
    };

    let footer = TranslatableFooter {
        translatable: &translatable,
        can_edit_translatable,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template: crate::util::EmptyTemplate,
        query,
        results,
        pagination,
        search_name: "translations",
        search_action,
        render_item,
    }).with_breadcrumbs(breadcrumbs).with_footer(footer);

    let status = template.status();
    (status, render_template(template))
}

// Step 1: Select language
#[derive(Template)]
#[template(path = "translations/new-1.html")]
#[allow(dead_code)]
struct NewTranslationStep1Template {
    current_user: Option<User>,
    error: Option<AppError>,
    translatable: Translatable,
    translatable_id: String,
    available_languages: Vec<Language>,
    previous_language_code: String,
    can_edit_translatable: bool,
    language: Option<Language>,
    can_edit_language: bool,
    will_create_audit_log: bool,
}

async fn new_translation_step_1_form(
    s: Session,
    State(_state): State<AppState>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let available_languages = attempt!(s, languages.find_all_by_user(user.id).await);

    let can_edit_translatable = translatable.created_by == user.id;

    let template = NewTranslationStep1Template {
        current_user: Some(user),
        error: None,
        translatable: translatable.clone(),
        translatable_id: translatable.slug,
        available_languages,
        previous_language_code: String::new(),
        can_edit_translatable,
        language: None,
        can_edit_language: false,
        will_create_audit_log: false,
    };

    okay(render_template(template))
}

// Step 2: Enter translation text
#[derive(Template)]
#[template(path = "translations/new-2.html")]
#[allow(dead_code)]
struct NewTranslationStep2Template {
    current_user: Option<User>,
    error: Option<AppError>,
    translatable_with_meta: TranslatableWithMeta,
    language: Language,
    language_with_contributors: LanguagesWithContributors,
    previous_translated_text: String,
    previous_translated_title: String,
    previous_ipa: String,
    previous_gloss: String,
    previous_notes: String,
    can_edit_translatable: bool,
    can_edit_language: bool,
    will_create_audit_log: bool,
    rendered_description: Option<String>,
}

#[derive(Deserialize)]
struct NewTranslationStep1Form {
    language_code: String,
}

async fn new_translation_step_1_submit(
    s: Session,
    State(state): State<AppState>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path(slug): Path<String>,
    Form(form): Form<NewTranslationStep1Form>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let can_edit_translatable = translatable.created_by == user.id;

    let Ok(language) = languages.find_by_code(&form.language_code).await else {
        let error = bad_request("Language not found");
        let available_languages = attempt!(s, languages.find_all_by_user(user.id).await);
        let template = NewTranslationStep1Template {
            current_user: Some(user),
            error: Some(error),
            translatable: translatable.clone(),
            translatable_id: translatable.slug,
            available_languages,
            previous_language_code: form.language_code,
            can_edit_translatable,
            language: None,
            can_edit_language: false,
            will_create_audit_log: false,
        };
        return (StatusCode::BAD_REQUEST, render_template(template));
    };

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

    let top_contributors = attempt!(
        s,
        contribution_stats
            .get_top_contributors(&language.id, 5)
            .await
    );
    let is_liked = attempt!(s, languages.is_liked(&user.id, &language.id).await);
    let language_with_contributors = LanguagesWithContributors {
        language: language.clone(),
        top_contributors,
        is_liked,
    };

    let rendered_description = if !translatable.description.is_empty() {
        crate::md::render_md(&translatable.description).ok()
    } else {
        None
    };

    let translatable_with_meta = attempt!(
        s,
        translatables.materialize(translatable, Some(&user)).await
    );

    let template = NewTranslationStep2Template {
        current_user: Some(user),
        error: None,
        translatable_with_meta,
        language,
        language_with_contributors,
        previous_translated_text: String::new(),
        previous_translated_title: String::new(),
        previous_ipa: String::new(),
        previous_gloss: String::new(),
        previous_notes: String::new(),
        can_edit_translatable,
        can_edit_language,
        will_create_audit_log,
        rendered_description,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewTranslationStep2Form {
    language_id: Uuid,
    translated_text: String,
    translated_title: Option<String>,
    ipa: Option<String>,
    gloss: Option<String>,
    notes: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn new_translation_step_2_submit(
    s: Session,
    State(state): State<AppState>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path(slug): Path<String>,
    Form(form): Form<NewTranslationStep2Form>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let can_edit_translatable = translatable.created_by == user.id;

    let language = attempt!(s, languages.find_by_id(form.language_id).await);

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

    let top_contributors = attempt!(
        s,
        contribution_stats
            .get_top_contributors(&language.id, 5)
            .await
    );
    let is_liked = attempt!(s, languages.is_liked(&user.id, &language.id).await);

    let create = CreateTranslation {
        translated_text: form.translated_text.clone(),
        translated_title: form.translated_title.clone(),
        ipa: form.ipa.clone(),
        gloss: form.gloss.clone(),
        notes: form.notes.clone(),
    };

    let create_result = translations
        .create(&user, translatable.id, language.id, create)
        .await;

    let rendered_description = if !translatable.description.is_empty() {
        crate::md::render_md(&translatable.description).ok()
    } else {
        None
    };

    let redirect_slug = translatable.slug.clone();

    match create_result {
        Ok(_) => {
            let redirect_url = format!(
                "/translatable/{}/translation/{}",
                redirect_slug, language.code
            );
            (
                StatusCode::SEE_OTHER,
                Redirect::to(&redirect_url).into_response(),
            )
        }
        Err(e) => {
            let language_with_contributors = LanguagesWithContributors {
                language: language.clone(),
                top_contributors,
                is_liked,
            };

            let translatable_with_meta = attempt!(
                s,
                translatables.materialize(translatable, Some(&user)).await
            );

            let template = NewTranslationStep2Template {
                current_user: Some(user),
                error: Some(e),
                translatable_with_meta,
                language,
                language_with_contributors,
                previous_translated_text: form.translated_text,
                previous_translated_title: form.translated_title.unwrap_or_default(),
                previous_ipa: form.ipa.unwrap_or_default(),
                previous_gloss: form.gloss.unwrap_or_default(),
                previous_notes: form.notes.unwrap_or_default(),
                can_edit_translatable,
                can_edit_language,
                will_create_audit_log,
                rendered_description,
            };

            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

// View translation
#[derive(Template)]
#[template(path = "translations/view.html")]
#[allow(clippy::struct_excessive_bools, dead_code)]
struct ViewTranslationTemplate {
    current_user: Option<User>,
    translatable_with_meta: TranslatableWithMeta,
    language: Language,
    translation: Translation,
    translation_creator: User,
    #[allow(dead_code)]
    current_user_has_permission: bool,
    can_edit_translatable: bool,
    can_edit_language: bool,
    can_edit_translation: bool,
    translation_is_liked: bool,
    json_ld: String,
    rendered_description: Option<String>,
    back: String,
    back_text: String,
}



async fn view_translation(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((slug, code)): Path<(String, String)>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let translation = attempt!(
        s,
        translations
            .find_by_translatable_and_language(translatable.id, language.id)
            .await
    );

    let translation_creator = attempt!(s, users.find_by_id(translation.created_by).await);

    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        return okay(
            render_embed(
                EmbedTarget::Discord,
                GenericEmbed {
                    title: format!("{} ({} translation)", translatable.title, language.name),
                    description: format!(
                        "{}\n\n⭐️ {}",
                        truncate_description(&translation.translated_text),
                        translation.like_count
                    ),
                    author: Some(translation_creator),
                    color: None,
                    url: format!(
                        "{}/translatable/{}/translation/{}",
                        &crate::CONFIG.public_url_base,
                        translatable.slug,
                        language.code
                    ),
                    image: None,
                },
            )
            .await
            .into_response(),
        );
    }

    let current_user_has_permission = if let Some(current_user) = s.user() {
        attempt!(
            s,
            permissions
                .find_by_user_and_language(current_user.id, language.id)
                .await
        )
        .is_some()
    } else {
        false
    };

    let can_edit_translatable = if let Some(current_user) = s.user() {
        translatable.created_by == current_user.id
    } else {
        false
    };

    let can_edit_language = if let Some(current_user) = s.user() {
        permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let translation_is_liked = if let Some(current_user) = s.user() {
        attempt!(
            s,
            translations
                .is_liked(&current_user.id, &translation.id)
                .await
        )
    } else {
        false
    };

    let can_edit_translation = current_user_has_permission;

    let json_ld = attempt!(
        s,
        serde_json::to_string(&attempt!(
            s,
            translations
                .as_json_ld(&translation, &translatable, &language)
                .await
        ))
        .map_err(Into::into)
    );

    let rendered_description = if !translatable.description.is_empty() {
        crate::md::render_md(&translatable.description).ok()
    } else {
        None
    };

    let translatable_with_meta = attempt!(
        s,
        translatables.materialize(translatable, s.user()).await
    );

    let back_text = back_query
        .back
        .as_ref()
        .and_then(|url| {
            if url.contains("/languages/") {
                Some("to language")
            } else if url.contains("/translatable/") {
                Some("to translatable")
            } else {
                None
            }
        })
        .unwrap_or("back");

    let back_url = back_query
        .back
        .clone()
        .unwrap_or_else(|| format!("/translatable/{}", &translatable_with_meta.translatable.slug));

    let template = ViewTranslationTemplate {
        current_user: s.user().cloned(),
        translatable_with_meta,
        language,
        translation,
        translation_creator,
        current_user_has_permission,
        can_edit_translatable,
        can_edit_language,
        can_edit_translation,
        translation_is_liked,
        json_ld,
        rendered_description,
        back: back_url,
        back_text: back_text.to_string(),
    };

    let body = render_template(template);
    okay(body)
}

// Edit translation
#[derive(Template)]
#[template(path = "translations/edit.html")]
#[allow(dead_code)]
#[allow(clippy::struct_excessive_bools)]
struct EditTranslationTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    translatable_with_meta: TranslatableWithMeta,
    language: Language,
    language_with_contributors: LanguagesWithContributors,
    translation: Translation,
    previous_translated_text: String,
    previous_translated_title: String,
    previous_ipa: String,
    previous_gloss: String,
    previous_notes: String,
    can_edit_translatable: bool,
    can_edit_language: bool,
    can_edit_translation: bool,
    will_create_audit_log: bool,
    rendered_description: Option<String>,
}

async fn edit_translation_form(
    s: Session,
    State(state): State<AppState>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    Path((slug, code)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let language = attempt!(s, languages.find_by_code(&code).await);

    let translation = attempt!(
        s,
        translations
            .find_by_translatable_and_language(translatable.id, language.id)
            .await
    );

    let can_edit_translatable = translatable.created_by == user.id;

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_language = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let can_edit_translation = is_admin_or_mod
        || permissions
            .find_by_user_and_language(user.id, language.id)
            .await
            .ok()
            .flatten()
            .is_some();

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let top_contributors = attempt!(
        s,
        ContributionStatsRepository::new(state.clone())
            .get_top_contributors(&language.id, 5)
            .await
    );
    let is_liked = attempt!(s, languages.is_liked(&user.id, &language.id).await);
    let language_with_contributors = LanguagesWithContributors {
        language: language.clone(),
        top_contributors,
        is_liked,
    };

    let rendered_description = if !translatable.description.is_empty() {
        crate::md::render_md(&translatable.description).ok()
    } else {
        None
    };

    let translatable_with_meta = attempt!(
        s,
        translatables.materialize(translatable, Some(&user)).await
    );

    let template = EditTranslationTemplate {
        current_user: Some(user),
        error: None,
        translatable_with_meta,
        language,
        language_with_contributors,
        translation: translation.clone(),
        previous_translated_text: translation.translated_text.clone(),
        previous_translated_title: translation.translated_title.clone(),
        previous_ipa: translation.ipa.clone(),
        previous_gloss: translation.gloss.clone(),
        previous_notes: translation.notes.clone(),
        can_edit_translatable,
        can_edit_language,
        can_edit_translation,
        will_create_audit_log,
        rendered_description,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditTranslationFormData {
    translated_text: String,
    translated_title: Option<String>,
    ipa: Option<String>,
    gloss: Option<String>,
    notes: Option<String>,
    language_id: Uuid,
}

#[allow(clippy::too_many_arguments)]
async fn edit_translation_submit(
    s: Session,
    State(state): State<AppState>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    Path(slug): Path<String>,
    Form(form): Form<EditTranslationFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let language = attempt!(s, languages.find_by_id(form.language_id).await);

    let can_edit_translatable = translatable.created_by == user.id;

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_language = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let can_edit_translation = is_admin_or_mod
        || permissions
            .find_by_user_and_language(user.id, language.id)
            .await
            .ok()
            .flatten()
            .is_some();

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let existing_translation = attempt!(
        s,
        translations
            .find_by_translatable_and_language(translatable.id, language.id)
            .await
    );
    let translation = translations
        .update(
            &user,
            existing_translation.id,
            UpdateTranslation {
                translated_text: Some(form.translated_text.clone()),
                translated_title: form.translated_title.clone(),
                ipa: form.ipa.clone(),
                gloss: form.gloss.clone(),
                notes: form.notes.clone(),
            },
        )
        .await;

    let top_contributors = attempt!(
        s,
        ContributionStatsRepository::new(state.clone())
            .get_top_contributors(&language.id, 5)
            .await
    );
    let is_liked = attempt!(s, languages.is_liked(&user.id, &language.id).await);
    let language_with_contributors = LanguagesWithContributors {
        language: language.clone(),
        top_contributors,
        is_liked,
    };

    let rendered_description = if !translatable.description.is_empty() {
        crate::md::render_md(&translatable.description).ok()
    } else {
        None
    };

    let redirect_slug = translatable.slug.clone();

    match translation {
        Ok(_) => {
            let redirect_url = format!(
                "/translatable/{}/translation/{}",
                redirect_slug, language.code
            );
            (
                StatusCode::SEE_OTHER,
                Redirect::to(&redirect_url).into_response(),
            )
        }
        Err(e) => {
            let translatable_with_meta = attempt!(
                s,
                translatables.materialize(translatable, Some(&user)).await
            );

            let template = EditTranslationTemplate {
                current_user: Some(user),
                error: Some(e),
                translatable_with_meta,
                language,
                translation: existing_translation,
                previous_translated_text: form.translated_text.clone(),
                previous_translated_title: form.translated_title.clone().unwrap_or_default(),
                previous_ipa: form.ipa.clone().unwrap_or_default(),
                previous_gloss: form.gloss.clone().unwrap_or_default(),
                previous_notes: form.notes.clone().unwrap_or_default(),
                language_with_contributors,
                can_edit_translatable,
                can_edit_language,
                can_edit_translation,
                will_create_audit_log,
                rendered_description,
            };
            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}
