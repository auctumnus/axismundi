use askama::Template;
use axum::{
    Router,
    extract::{Path, Query, State},
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
        language_families::LanguageFamilyRepository,
        language_family_members::{LanguageFamilyMemberRepository, MemberWithLanguages},
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::{
            MemberTarget, NewSoundChangeSet, SearchSoundChangeSets, SoundChangeSet,
            SoundChangeSetRepository, UpdateSoundChangeSet,
        },
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{
        AppState, BackQuery,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/languages/{code}/sound-change-sets/new", post(new_submit))
        .route(
            "/languages/{code}/sound-change-sets/{id}/edit",
            post(edit_meta_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/delete",
            post(delete_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/set-ipa-estimator",
            post(set_ipa_estimator_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/unset-ipa-estimator",
            post(unset_ipa_estimator_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/save",
            post(save_changes),
        )
        .route("/sound-change-sets/save", post(save_to_new_submit))
        .route("/sound-change-sets/{id}/save", post(save_global))
        .route("/sound-change-sets/{id}/reassign", post(reassign_submit))
        .route(
            "/language-families/{fam_code}/members/{member_id}/sound-change-sets/new",
            post(new_for_member_submit),
        )
        .route(
            "/language-families/{fam_code}/members/{member_id}/sound-change-sets/{id}/edit",
            post(edit_meta_for_member_submit),
        )
        .route(
            "/language-families/{fam_code}/members/{member_id}/sound-change-sets/{id}/delete",
            post(delete_for_member_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{code}/sound-change-sets", get(search))
        .route("/languages/{code}/sound-change-sets/new", get(new_form))
        .route("/languages/{code}/sound-change-sets/{id}", get(view))
        .route(
            "/languages/{code}/sound-change-sets/{id}/edit",
            get(edit_meta_form),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/delete",
            get(delete_form),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/set-ipa-estimator",
            get(set_ipa_estimator_form),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/unset-ipa-estimator",
            get(unset_ipa_estimator_form),
        )
        .route("/sound-change-sets/run", get(run_form).post(run_submit))
        .route("/sound-change-sets/save-to-new", post(save_to_new_form))
        .route("/sound-change-sets/{id}", get(view_global))
        .route("/sound-change-sets/{id}/reassign", get(reassign_form))
        .route(
            "/language-families/{fam_code}/members/{member_id}/sound-change-sets/new",
            get(new_for_member_form),
        )
        .route(
            "/language-families/{fam_code}/members/{member_id}/sound-change-sets/{id}/edit",
            get(edit_meta_for_member_form),
        )
        .route(
            "/language-families/{fam_code}/members/{member_id}/sound-change-sets/{id}/delete",
            get(delete_for_member_form),
        );

    (secure_routes, normal_routes)
}

// --- shared types ---

struct SoundChangeSetWithMeta {
    sound_change_set: SoundChangeSet,
    author: User,
}

struct OutputPair {
    input: String,
    output: String,
}

// --- templates ---

#[derive(Template)]
#[template(path = "sound-change-sets/fragments/card.html")]
#[allow(dead_code)]
struct ScsPreviewCard {
    set: SoundChangeSet,
    author: User,
    language: Language,
    can_edit_language: bool,
    back_url: String,
}

#[derive(Template)]
#[template(path = "sound-change-sets/fragments/list_header.html")]
struct ScsSearchHeader {
    can_edit_language: bool,
    language_code: String,
}

#[derive(Template)]
#[template(path = "sound-change-sets/fragments/query.html")]
struct ScsSearchQueryTemplate {
    author: Option<String>,
}

#[derive(Template)]
#[template(path = "sound-change-sets/new.html")]
#[allow(dead_code)]
struct NewTemplate {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    will_create_audit_log: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/edit-meta.html")]
#[allow(dead_code)]
struct EditMetaTemplate {
    current_user: Option<User>,
    language: Language,
    sound_change_set: SoundChangeSet,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    will_create_audit_log: bool,
    can_edit_language: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/delete.html")]
#[allow(dead_code)]
struct DeleteTemplate {
    current_user: Option<User>,
    language: Language,
    set: SoundChangeSet,
    will_create_audit_log: bool,
    can_edit_language: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/set-ipa-estimator.html")]
#[allow(dead_code)]
struct SetIpaEstimatorTemplate {
    current_user: Option<User>,
    language: Language,
    set: SoundChangeSet,
    can_edit_language: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/unset-ipa-estimator.html")]
#[allow(dead_code)]
struct UnsetIpaEstimatorTemplate {
    current_user: Option<User>,
    language: Language,
    set: SoundChangeSet,
    can_edit_language: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/run.html")]
#[allow(dead_code)]
struct RunTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    associated_sound_set: Option<(
        SoundChangeSetWithMeta,
        Option<LanguagesWithContributors>,
        bool,
    )>,
    previous_input_words: String,
    previous_changes: String,
    previous_start_at: String,
    previous_stop_before: String,
    previous_trace_words: String,
    rendered_output: Vec<OutputPair>,
    rendered_traces: Vec<String>,
    will_create_audit_log: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/save-to-new.html")]
#[allow(dead_code)]
struct SaveToNewTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    available_languages: Vec<Language>,
    available_members: Vec<MemberTarget>,
    previous_language_code: String,
    previous_member_id: String,
    previous_name: String,
    previous_description: String,
    changes: String,
}

// --- form data ---

#[derive(Deserialize)]
struct NewFormData {
    name: String,
    description: String,
}

#[derive(Deserialize)]
struct EditMetaFormData {
    name: String,
    description: String,
}

#[derive(Deserialize)]
struct RunFormData {
    input_words: String,
    changes: String,
    #[serde(default)]
    start_at: String,
    #[serde(default)]
    stop_before: String,
    #[serde(default)]
    trace_words: String,
}

#[derive(Deserialize)]
struct SaveChangesFormData {
    changes: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SaveToNewFormData {
    changes: String,
    #[serde(default)]
    input_words: String,
}

#[derive(Deserialize)]
struct SaveToNewSubmitFormData {
    #[serde(default)]
    language_code: String,
    #[serde(default)]
    member_id: String,
    name: String,
    description: String,
    changes: String,
}

#[derive(Deserialize)]
struct RunQueryParams {
    set: Option<Uuid>,
}

// --- helpers ---

async fn build_language_context(
    s: &Session,
    languages: &LanguageRepository,
    permissions: &LanguagePermissionRepository,
    contribution_stats: &ContributionStatsRepository,
    language: Language,
) -> Result<(LanguagesWithContributors, bool, bool), AppError> {
    let top_contributors = contribution_stats
        .get_top_contributors(&language.id, 5)
        .await?;
    let is_liked = if let Some(user) = s.user() {
        languages
            .is_liked(&user.id, &language.id)
            .await
            .unwrap_or(false)
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

    let can_delete_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let lwc = LanguagesWithContributors {
        language,
        top_contributors,
        is_liked,
    };

    Ok((lwc, can_edit_language, can_delete_language))
}

// --- handlers ---

async fn search(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path(code): Path<String>,
    axum::extract::Query(query): axum::extract::Query<SearchSoundChangeSets>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&code).await);

    let search_action = format!("/languages/{}/sound-change-sets", code);
    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let (language_with_contributors, can_edit_language, can_delete_language) = attempt!(
        s,
        build_language_context(&s, &languages, &permissions, &contribution_stats, language).await
    );

    let lang = &language_with_contributors.language;

    let results = match sets.search(lang, pagination.clone(), query.clone()).await {
        Ok(response) => {
            let mut items = Vec::with_capacity(response.items.len());
            for set in response.items {
                let author = attempt!(s, users.find_by_id(set.created_by).await);
                items.push(SoundChangeSetWithMeta {
                    sound_change_set: set,
                    author,
                });
            }
            Ok(crate::pagination::PaginatedResponse {
                items,
                total: response.total,
                limit: response.limit,
                offset: response.offset,
                has_more: response.has_more,
            })
        }
        Err(e) => Err(e),
    };

    let lang_clone = language_with_contributors.language.clone();
    let render_item = move |item: &SoundChangeSetWithMeta| ScsPreviewCard {
        set: item.sound_change_set.clone(),
        author: item.author.clone(),
        language: lang_clone.clone(),
        can_edit_language,
        back_url: back_url.clone(),
    };

    let header = ScsSearchHeader {
        can_edit_language,
        language_code: language_with_contributors.language.code.clone(),
    };

    let query_template = ScsSearchQueryTemplate {
        author: query.author.clone(),
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
        search_name: "sound change sets",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();
    (status, render_template(template))
}

async fn view(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path((code, id)): Path<(String, Uuid)>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let author = attempt!(s, users.find_by_id(set.created_by).await);

    let (language_with_contributors, can_edit_language, _can_delete_language) = attempt!(
        s,
        build_language_context(&s, &languages, &permissions, &contribution_stats, language).await
    );

    let rendered_description = if !set.description.is_empty() {
        attempt!(s, render_md(&set.description).map_err(Into::into))
    } else {
        String::new()
    };

    let existing_estimator = attempt!(
        s,
        languages
            .get_ipa_estimator(language_with_contributors.language.id)
            .await
    );
    let language_has_ipa_estimator = existing_estimator.is_some();

    let template = ViewGlobalTemplate {
        current_user: s.user().cloned(),
        set,
        author,
        rendered_description,
        can_edit: can_edit_language,
        back: back_query.back.unwrap_or_default(),
        language: Some(language_with_contributors),
        member: None,
        language_has_ipa_estimator,
    };

    okay(render_template(template))
}

async fn new_form(
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

    let template = NewTemplate {
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

async fn new_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    form: axum::Form<NewFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    if form.name.trim().is_empty() {
        let template = NewTemplate {
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

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    match sets
        .create_for_language(
            &user,
            &language,
            NewSoundChangeSet {
                name: form.name.clone(),
                description,
                changes: String::new(),
            },
        )
        .await
    {
        Ok(created) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/run?set={}", created.id)).into_response(),
        ),
        Err(e) => {
            let template = NewTemplate {
                current_user: Some(user),
                language,
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

async fn edit_meta_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = EditMetaTemplate {
        current_user: Some(user),
        language,
        previous_name: set.name.clone(),
        previous_description: set.description.clone(),
        sound_change_set: set,
        error: None,
        will_create_audit_log,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn edit_meta_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
    form: axum::Form<EditMetaFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

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

    let updates = UpdateSoundChangeSet {
        name: if form.name == set.name {
            None
        } else {
            Some(form.name.clone())
        },
        description: if description.as_deref().unwrap_or("") == set.description {
            None
        } else {
            description
        },
        changes: None,
    };

    match sets.update(&user, &set.id, updates).await {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/sound-change-sets/{}",
                code, updated.id
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = EditMetaTemplate {
                current_user: Some(user),
                language,
                sound_change_set: set,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                will_create_audit_log,
                can_edit_language,
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
    sets: SoundChangeSetRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeleteTemplate {
        current_user: Some(user),
        language,
        set,
        will_create_audit_log,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn delete_submit(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let _language = attempt!(s, languages.find_by_code(&code).await);

    match sets.delete(&user, &id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/sound-change-sets", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

async fn run_form(
    s: Session,
    State(state): State<AppState>,
    sets: SoundChangeSetRepository,
    users: UserRepository,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    axum::extract::Query(params): axum::extract::Query<RunQueryParams>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    let will_create_audit_log = if let Some(user) = &current_user {
        if let Some(set_id) = params.set {
            if let Ok(Some(set)) = sets.get(set_id).await {
                if let Some(language_id) = set.language_id {
                    crate::util::will_create_audit_log_for_language(&state, user, language_id).await
                } else {
                    // TODO: audit log for family-owned SCS
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let associated_sound_set = if let Some(set_id) = params.set {
        match sets.get(set_id).await {
            Ok(Some(set)) => {
                let author = attempt!(s, users.find_by_id(set.created_by).await);
                let (lwc, can_edit) = if let Some(language_id) = set.language_id {
                    let language = attempt!(s, languages.find_by_id(language_id).await);
                    let (lwc, can_edit, _) = attempt!(
                        s,
                        build_language_context(
                            &s,
                            &languages,
                            &permissions,
                            &contribution_stats,
                            language
                        )
                        .await
                    );
                    (Some(lwc), can_edit)
                } else {
                    let can_edit = if let Some(user) = s.user() {
                        sets.can_edit(user.id, &set).await.unwrap_or(false)
                    } else {
                        false
                    };
                    (None, can_edit)
                };
                let meta = SoundChangeSetWithMeta {
                    sound_change_set: set,
                    author,
                };
                Some((meta, lwc, can_edit))
            }
            _ => None,
        }
    } else {
        None
    };

    let previous_changes = associated_sound_set
        .as_ref()
        .map(|(m, _, _)| m.sound_change_set.changes.clone())
        .unwrap_or_default();

    let template = RunTemplate {
        current_user,
        error: None,
        associated_sound_set,
        previous_input_words: String::new(),
        previous_changes,
        previous_start_at: String::new(),
        previous_stop_before: String::new(),
        previous_trace_words: String::new(),
        rendered_output: vec![],
        rendered_traces: vec![],
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn run_submit(
    s: Session,
    State(state): State<AppState>,
    sets: SoundChangeSetRepository,
    users: UserRepository,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    axum::extract::Query(params): axum::extract::Query<RunQueryParams>,
    form: axum::Form<RunFormData>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    let will_create_audit_log = if let Some(user) = &current_user {
        if let Some(set_id) = params.set {
            if let Ok(Some(set)) = sets.get(set_id).await {
                if let Some(language_id) = set.language_id {
                    crate::util::will_create_audit_log_for_language(&state, user, language_id).await
                } else {
                    // TODO: audit log for family-owned SCS
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let associated_sound_set = if let Some(set_id) = params.set {
        match sets.get(set_id).await {
            Ok(Some(set)) => {
                let author = attempt!(s, users.find_by_id(set.created_by).await);
                let (lwc, can_edit) = if let Some(language_id) = set.language_id {
                    let language = attempt!(s, languages.find_by_id(language_id).await);
                    let (lwc, can_edit, _) = attempt!(
                        s,
                        build_language_context(
                            &s,
                            &languages,
                            &permissions,
                            &contribution_stats,
                            language
                        )
                        .await
                    );
                    (Some(lwc), can_edit)
                } else {
                    let can_edit = if let Some(user) = s.user() {
                        sets.can_edit(user.id, &set).await.unwrap_or(false)
                    } else {
                        false
                    };
                    (None, can_edit)
                };
                let meta = SoundChangeSetWithMeta {
                    sound_change_set: set,
                    author,
                };
                Some((meta, lwc, can_edit))
            }
            _ => None,
        }
    } else {
        None
    };

    let input_words: Vec<String> = form
        .input_words
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let start_at = if form.start_at.trim().is_empty() {
        None
    } else {
        Some(form.start_at.clone())
    };

    let stop_before = if form.stop_before.trim().is_empty() {
        None
    } else {
        Some(form.stop_before.clone())
    };

    let trace_words: Option<Vec<String>> = {
        let words: Vec<String> = form
            .trace_words
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if words.is_empty() { None } else { Some(words) }
    };

    if input_words.is_empty() || form.changes.trim().is_empty() {
        let template = RunTemplate {
            current_user,
            error: Some(crate::err::bad_request(
                "Both input words and sound change rules are required.",
            )),
            associated_sound_set,
            previous_input_words: form.input_words.clone(),
            previous_changes: form.changes.clone(),
            previous_start_at: form.start_at.clone(),
            previous_stop_before: form.stop_before.clone(),
            previous_trace_words: form.trace_words.clone(),
            rendered_output: vec![],
            rendered_traces: vec![],
            will_create_audit_log,
        };
        return (StatusCode::BAD_REQUEST, render_template(template));
    }

    match crate::lexurgy::run_sound_changes(
        form.changes.clone(),
        input_words.clone(),
        start_at,
        stop_before,
        trace_words,
    )
    .await
    {
        Ok(Ok(response)) => {
            let rendered_output: Vec<OutputPair> = input_words
                .iter()
                .zip(response.output_words.iter())
                .map(|(input, output)| OutputPair {
                    input: input.clone(),
                    output: output.clone(),
                })
                .collect();

            let rendered_traces = if let Some(traces) = response.traces {
                traces
                    .into_iter()
                    .map(|(input, steps)| {
                        let steps_html: String = steps
                            .iter()
                            .enumerate()
                            .map(|(i, step)| {
                                let step_input = if i == 0 { &input } else { &steps[i - 1].output };
                                format!(
                                    "<tr><td>{}</td><td>{step_input}</td><td>{}</td></tr>",
                                    step.rule, step.output
                                )
                            })
                            .collect();

                        format!(
                            "<table>
                            <caption>Trace for \"{input}\"</caption>
                            <thead>
                                <tr>
                                    <th>Applied Rule</th>
                                    <th>Input</th>
                                    <th>Output</th>
                                </tr>
                            </thead>
                            <tbody>
                                {steps_html}
                            </tbody>
                        </table>"
                        )
                    })
                    .collect()
            } else {
                vec![]
            };

            let template = RunTemplate {
                current_user,
                error: None,
                associated_sound_set,
                previous_input_words: form.input_words.clone(),
                previous_changes: form.changes.clone(),
                previous_start_at: form.start_at.clone(),
                previous_stop_before: form.stop_before.clone(),
                previous_trace_words: form.trace_words.clone(),
                rendered_output,
                rendered_traces,
                will_create_audit_log,
            };
            okay(render_template(template))
        }
        Ok(Err(lexurgy_error)) => {
            let template = RunTemplate {
                current_user,
                error: Some(crate::err::bad_request(format!("{}", lexurgy_error))),
                associated_sound_set,
                previous_input_words: form.input_words.clone(),
                previous_changes: form.changes.clone(),
                previous_start_at: form.start_at.clone(),
                previous_stop_before: form.stop_before.clone(),
                previous_trace_words: form.trace_words.clone(),
                rendered_output: vec![],
                rendered_traces: vec![],
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
        Err(e) => {
            let template = RunTemplate {
                current_user,
                error: Some(e),
                associated_sound_set,
                previous_input_words: form.input_words.clone(),
                previous_changes: form.changes.clone(),
                previous_start_at: form.start_at.clone(),
                previous_stop_before: form.stop_before.clone(),
                previous_trace_words: form.trace_words.clone(),
                rendered_output: vec![],
                rendered_traces: vec![],
                will_create_audit_log,
            };
            (StatusCode::INTERNAL_SERVER_ERROR, render_template(template))
        }
    }
}

async fn save_changes(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    Path((code, id)): Path<(String, Uuid)>,
    form: axum::Form<SaveChangesFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let _language = attempt!(s, languages.find_by_code(&code).await);

    let updates = UpdateSoundChangeSet {
        name: None,
        description: None,
        changes: Some(form.changes.clone()),
    };

    match sets.update(&user, &id, updates).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/run?set={}", id)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

async fn save_to_new_form(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    form: axum::Form<SaveToNewFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);
    let available_members = sets
        .find_member_targets_for_user(user.id)
        .await
        .unwrap_or_default();

    let template = SaveToNewTemplate {
        current_user: Some(user),
        error: None,
        available_languages,
        available_members,
        previous_language_code: String::new(),
        previous_member_id: String::new(),
        previous_name: String::new(),
        previous_description: String::new(),
        changes: form.changes.clone(),
    };

    okay(render_template(template))
}

async fn save_to_new_submit(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    members: LanguageFamilyMemberRepository,
    form: axum::Form<SaveToNewSubmitFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    let new_set = NewSoundChangeSet {
        name: form.name.clone(),
        description,
        changes: form.changes.clone(),
    };

    // dispatch: member_id takes priority over language_code
    let member_id = form.member_id.trim().parse::<uuid::Uuid>().ok();

    let name_empty = form.name.trim().is_empty();
    let both_empty = form.language_code.trim().is_empty() && member_id.is_none();

    if name_empty || both_empty {
        let available_languages = languages
            .list_editable_by_user(user.id)
            .await
            .unwrap_or_default();
        let available_members = sets
            .find_member_targets_for_user(user.id)
            .await
            .unwrap_or_default();
        let template = SaveToNewTemplate {
            current_user: Some(user),
            error: Some(crate::err::bad_request(
                "A name and either a language or family member are required.",
            )),
            available_languages,
            available_members,
            previous_language_code: form.language_code.clone(),
            previous_member_id: form.member_id.clone(),
            previous_name: form.name.clone(),
            previous_description: form.description.clone(),
            changes: form.changes.clone(),
        };
        return (StatusCode::BAD_REQUEST, render_template(template));
    }

    let result = if let Some(mid) = member_id {
        let member = attempt!(s, members.find_by_id(mid).await);
        sets.create_for_member(&user, &member, new_set).await
    } else {
        let language = attempt!(s, languages.find_by_code(&form.language_code).await);
        sets.create_for_language(&user, &language, new_set).await
    };

    match result {
        Ok(created) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/run?set={}", created.id)).into_response(),
        ),
        Err(e) => {
            let available_languages = languages
                .list_editable_by_user(user.id)
                .await
                .unwrap_or_default();
            let available_members = sets
                .find_member_targets_for_user(user.id)
                .await
                .unwrap_or_default();
            let template = SaveToNewTemplate {
                current_user: Some(user),
                error: Some(e),
                available_languages,
                available_members,
                previous_language_code: form.language_code.clone(),
                previous_member_id: form.member_id.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                changes: form.changes.clone(),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

// --- global (non-language-scoped) handlers ---

async fn save_global(
    s: Session,
    sets: SoundChangeSetRepository,
    Path(id): Path<Uuid>,
    form: axum::Form<SaveChangesFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let updates = UpdateSoundChangeSet {
        name: None,
        description: None,
        changes: Some(form.changes.clone()),
    };

    match sets.update(&user, &id, updates).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/run?set={}", id)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

// --- view_global ---

#[derive(Template)]
#[template(path = "sound-change-sets/view-global.html")]
#[allow(dead_code)]
struct ViewGlobalTemplate {
    current_user: Option<User>,
    set: SoundChangeSet,
    author: User,
    rendered_description: String,
    can_edit: bool,
    back: String,
    language: Option<LanguagesWithContributors>,
    member: Option<MemberWithLanguages>,
    language_has_ipa_estimator: bool,
}

async fn view_global(
    s: Session,
    sets: SoundChangeSetRepository,
    users: UserRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    family_permissions: LanguageFamilyPermissionRepository,
    Path(id): Path<Uuid>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let author = attempt!(s, users.find_by_id(set.created_by).await);

    let can_edit = if let Some(user) = s.user() {
        sets.can_edit(user.id, &set).await.unwrap_or(false)
    } else {
        false
    };

    let rendered_description = if !set.description.is_empty() {
        attempt!(s, render_md(&set.description).map_err(Into::into))
    } else {
        String::new()
    };

    let language = if let Some(language_id) = set.language_id {
        let lang = attempt!(s, languages.find_by_id(language_id).await);
        let top_contributors = attempt!(
            s,
            contribution_stats
                .get_top_contributors(&language_id, 5)
                .await
        );
        let is_liked = if let Some(user) = s.user() {
            languages
                .is_liked(&user.id, &language_id)
                .await
                .unwrap_or(false)
        } else {
            false
        };
        Some(LanguagesWithContributors {
            language: lang,
            top_contributors,
            is_liked,
        })
    } else {
        None
    };

    let language_has_ipa_estimator = if let Some(ref lang) = language {
        let existing = attempt!(s, languages.get_ipa_estimator(lang.language.id).await);
        existing.is_some()
    } else {
        false
    };

    let member = if let Some(member_id) = set.member_id {
        let raw = attempt!(s, members.find_by_id(member_id).await);
        Some(attempt!(s, members.materialize(raw).await))
    } else {
        None
    };

    let _ = (language_families, permissions, family_permissions); // used indirectly above

    let template = ViewGlobalTemplate {
        current_user: s.user().cloned(),
        set,
        author,
        rendered_description,
        can_edit,
        back: back_query.back.unwrap_or_default(),
        language,
        member,
        language_has_ipa_estimator,
    };

    okay(render_template(template))
}

// --- reassign ---

#[derive(Template)]
#[template(path = "sound-change-sets/reassign.html")]
#[allow(dead_code)]
struct ReassignTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    set: SoundChangeSet,
    member_targets: Vec<MemberTarget>,
    language_targets: Vec<Language>,
    is_ipa_estimator: bool,
    language: Option<Language>,
}

#[derive(Deserialize)]
struct ReassignFormData {
    #[serde(default)]
    target_member_id: String,
    #[serde(default)]
    target_language_id: String,
}

async fn reassign_form(
    s: Session,
    sets: SoundChangeSetRepository,
    languages: LanguageRepository,
    Path(id): Path<Uuid>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let (member_targets, language, language_targets) = match set.language_id {
        Some(language_id) => {
            let targets = attempt!(
                s,
                sets.find_available_member_targets(user.id, language_id)
                    .await
            );

            (
                targets,
                Some(attempt!(s, languages.find_by_id(language_id).await)),
                vec![],
            )
        }
        None => {
            let langs = languages
                .list_editable_by_user(user.id)
                .await
                .unwrap_or_default();
            (vec![], None, langs)
        }
    };

    let is_ipa_estimator = attempt!(s, sets.is_ipa_estimator(set.id).await);

    let template = ReassignTemplate {
        current_user: Some(user),
        error: None,
        set,
        member_targets,
        language_targets,
        is_ipa_estimator,
        language,
    };

    okay(render_template(template))
}

async fn reassign_submit(
    s: Session,
    sets: SoundChangeSetRepository,
    languages: LanguageRepository,
    Path(id): Path<Uuid>,
    form: axum::Form<ReassignFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let member_id = form.target_member_id.trim().parse::<Uuid>().ok();
    let language_id = form.target_language_id.trim().parse::<Uuid>().ok();

    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let is_ipa_estimator = attempt!(s, sets.is_ipa_estimator(set.id).await);

    let result = match (member_id, language_id) {
        (Some(mid), _) => sets.reassign_to_member(&user, id, mid).await,
        (_, Some(lid)) => sets.reassign_to_language(&user, id, lid).await,
        _ => {
            let (member_targets, language, language_targets) = match set.language_id {
                Some(language_id) => (
                    sets.find_available_member_targets(user.id, language_id)
                        .await
                        .unwrap_or_default(),
                    Some(attempt!(s, languages.find_by_id(language_id).await)),
                    vec![],
                ),
                None => (
                    vec![],
                    None,
                    languages
                        .list_editable_by_user(user.id)
                        .await
                        .unwrap_or_default(),
                ),
            };
            let template = ReassignTemplate {
                current_user: Some(user),
                error: Some(crate::err::bad_request("Please select a target.")),
                set,
                member_targets,
                language_targets,
                is_ipa_estimator,
                language,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    match result {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/{}", updated.id)).into_response(),
        ),
        Err(e) => {
            let (member_targets, language, language_targets) = match set.language_id {
                Some(language_id) => (
                    sets.find_available_member_targets(user.id, language_id)
                        .await
                        .unwrap_or_default(),
                    Some(attempt!(s, languages.find_by_id(language_id).await)),
                    vec![],
                ),
                None => (
                    vec![],
                    None,
                    languages
                        .list_editable_by_user(user.id)
                        .await
                        .unwrap_or_default(),
                ),
            };
            let template = ReassignTemplate {
                current_user: Some(user),
                error: Some(e),
                set,
                member_targets,
                language_targets,
                is_ipa_estimator,
                language,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

// --- edit-meta and delete for member ---

#[derive(Template)]
#[template(path = "sound-change-sets/edit-meta-for-member.html")]
#[allow(dead_code)]
struct EditMetaForMemberTemplate {
    current_user: Option<User>,
    fam_code: String,
    member: MemberWithLanguages,
    sound_change_set: SoundChangeSet,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
    will_create_audit_log: bool,
    can_edit: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/delete-for-member.html")]
#[allow(dead_code)]
struct DeleteForMemberTemplate {
    current_user: Option<User>,
    fam_code: String,
    member: MemberWithLanguages,
    set: SoundChangeSet,
    will_create_audit_log: bool,
    can_edit: bool,
}

// --- new for member ---

#[derive(Template)]
#[template(path = "sound-change-sets/new-for-member.html")]
#[allow(dead_code)]
struct NewForMemberTemplate {
    current_user: Option<User>,
    fam_code: String,
    member: MemberWithLanguages,
    error: Option<AppError>,
    previous_name: String,
    previous_description: String,
}

async fn new_for_member_form(
    s: Session,
    members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    sets: SoundChangeSetRepository,
    Path((fam_code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let _user = get_user!(s);
    let _ = language_families;

    let raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(raw).await);

    // guard: if a SCS already exists, redirect to it
    if let Ok(Some(existing)) = sets.get_for_member(member_id).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/{}", existing.id)).into_response(),
        );
    }

    let template = NewForMemberTemplate {
        current_user: s.user().cloned(),
        fam_code,
        member,
        error: None,
        previous_name: String::new(),
        previous_description: String::new(),
    };

    okay(render_template(template))
}

async fn new_for_member_submit(
    s: Session,
    members: LanguageFamilyMemberRepository,
    sets: SoundChangeSetRepository,
    Path((fam_code, member_id)): Path<(String, Uuid)>,
    form: axum::Form<NewFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let _ = fam_code;

    let raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(raw).await);

    if form.name.trim().is_empty() {
        let template = NewForMemberTemplate {
            current_user: Some(user),
            fam_code: member.family.code.clone(),
            member,
            error: Some(crate::err::bad_request("Name is required.")),
            previous_name: form.name.clone(),
            previous_description: form.description.clone(),
        };
        return (StatusCode::BAD_REQUEST, render_template(template));
    }

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    match sets
        .create_for_member(
            &user,
            &member.member,
            NewSoundChangeSet {
                name: form.name.clone(),
                description,
                changes: String::new(),
            },
        )
        .await
    {
        Ok(created) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/run?set={}", created.id)).into_response(),
        ),
        Err(e) => {
            let template = NewForMemberTemplate {
                current_user: Some(user),
                fam_code: member.family.code.clone(),
                member,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

async fn edit_meta_for_member_form(
    s: Session,
    State(state): State<AppState>,
    members: LanguageFamilyMemberRepository,
    sets: SoundChangeSetRepository,
    family_permissions: LanguageFamilyPermissionRepository,
    Path((fam_code, member_id, id)): Path<(String, Uuid, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(raw).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit = family_permissions
        .has_permission(user.id, member.family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, member.family.id).await;

    let template = EditMetaForMemberTemplate {
        current_user: Some(user),
        fam_code,
        previous_name: set.name.clone(),
        previous_description: set.description.clone(),
        sound_change_set: set,
        member,
        error: None,
        will_create_audit_log,
        can_edit,
    };

    okay(render_template(template))
}

async fn edit_meta_for_member_submit(
    s: Session,
    State(state): State<AppState>,
    members: LanguageFamilyMemberRepository,
    sets: SoundChangeSetRepository,
    family_permissions: LanguageFamilyPermissionRepository,
    Path((fam_code, member_id, id)): Path<(String, Uuid, Uuid)>,
    form: axum::Form<EditMetaFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(raw).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit = family_permissions
        .has_permission(user.id, member.family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, member.family.id).await;

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    let updates = UpdateSoundChangeSet {
        name: if form.name == set.name {
            None
        } else {
            Some(form.name.clone())
        },
        description: if description.as_deref().unwrap_or("") == set.description {
            None
        } else {
            description
        },
        changes: None,
    };

    match sets.update(&user, &set.id, updates).await {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/{}", updated.id)).into_response(),
        ),
        Err(e) => {
            let template = EditMetaForMemberTemplate {
                current_user: Some(user),
                fam_code,
                member,
                sound_change_set: set,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                will_create_audit_log,
                can_edit,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

async fn delete_for_member_form(
    s: Session,
    State(state): State<AppState>,
    members: LanguageFamilyMemberRepository,
    sets: SoundChangeSetRepository,
    family_permissions: LanguageFamilyPermissionRepository,
    Path((fam_code, member_id, id)): Path<(String, Uuid, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(raw).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit = family_permissions
        .has_permission(user.id, member.family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, member.family.id).await;

    let template = DeleteForMemberTemplate {
        current_user: Some(user),
        fam_code,
        member,
        set,
        will_create_audit_log,
        can_edit,
    };

    okay(render_template(template))
}

async fn delete_for_member_submit(
    s: Session,
    sets: SoundChangeSetRepository,
    Path((fam_code, member_id, id)): Path<(String, Uuid, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    match sets.delete(&user, &id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/language-families/{}/members/{}",
                fam_code, member_id
            ))
            .into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

async fn set_ipa_estimator_form(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let template = SetIpaEstimatorTemplate {
        current_user: Some(user),
        language,
        set,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn unset_ipa_estimator_form(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let template = UnsetIpaEstimatorTemplate {
        current_user: Some(user),
        language,
        set,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn set_ipa_estimator_submit(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };
    attempt!(
        s,
        languages
            .set_ipa_estimator(&user, language.id, Some(set.id))
            .await
    );
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/languages/{}/sound-change-sets/{}", code, id)).into_response(),
    )
}

async fn unset_ipa_estimator_submit(
    s: Session,
    languages: LanguageRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    attempt!(
        s,
        languages.set_ipa_estimator(&user, language.id, None).await
    );
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/languages/{}/sound-change-sets/{}", code, id)).into_response(),
    )
}
