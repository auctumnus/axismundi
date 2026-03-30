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
    controller::html::{LanguagesWithContributors, okay, render_generic_error, render_template},
    err::AppError,
    get_user,
    md::render_md,
    model::{
        contribution_stats::ContributionStatsRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::{
            NewSoundChangeSet, SearchSoundChangeSets, SoundChangeSet,
            SoundChangeSetRepository, UpdateSoundChangeSet,
        },
        users::{User, UserRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/sound-change-sets/new",
            post(new_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/edit",
            post(edit_meta_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/delete",
            post(delete_submit),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/save",
            post(save_changes),
        )
        .route(
            "/sound-change-sets/save",
            post(save_to_new_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/sound-change-sets",
            get(search),
        )
        .route(
            "/languages/{code}/sound-change-sets/new",
            get(new_form),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}",
            get(view),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/edit",
            get(edit_meta_form),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/delete",
            get(delete_form),
        )
        .route(
            "/sound-change-sets/run",
            get(run_form).post(run_submit),
        )
        .route(
            "/sound-change-sets/save-to-new",
            post(save_to_new_form),
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
#[template(path = "sound-change-sets/search.html")]
#[allow(dead_code)]
struct SearchTemplate {
    current_user: Option<User>,
    language: LanguagesWithContributors,
    error: Option<AppError>,
    previous_query: SearchSoundChangeSets,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<SoundChangeSet>>,
    sets_with_meta: Vec<SoundChangeSetWithMeta>,
    can_edit_language: bool,
    can_delete_language: bool,
}

#[derive(Template)]
#[template(path = "sound-change-sets/view.html")]
#[allow(dead_code)]
struct ViewTemplate {
    current_user: Option<User>,
    language: LanguagesWithContributors,
    set: SoundChangeSet,
    author: User,
    rendered_description: String,
    can_edit_language: bool,
    can_delete_language: bool,
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
#[template(path = "sound-change-sets/run.html")]
#[allow(dead_code)]
struct RunTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    associated_sound_set: Option<(SoundChangeSetWithMeta, LanguagesWithContributors, bool)>,
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
    previous_language_code: String,
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
struct SaveToNewFormData {
    changes: String,
    #[serde(default)]
    input_words: String,
}

#[derive(Deserialize)]
struct SaveToNewSubmitFormData {
    language_code: String,
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
    axum::extract::Query(pagination): axum::extract::Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&code).await);

    let query = SearchSoundChangeSets {
        q: query.q.and_then(|q| {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        ..query
    };

    let (language_with_contributors, can_edit_language, can_delete_language) = attempt!(
        s,
        build_language_context(&s, &languages, &permissions, &contribution_stats, language).await
    );

    let results = match sets
        .search(
            &language_with_contributors.language,
            pagination.clone(),
            query.clone(),
        )
        .await
    {
        Ok(res) => res,
        Err(e) => {
            let template = SearchTemplate {
                current_user,
                language: language_with_contributors,
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
                sets_with_meta: vec![],
                can_edit_language,
                can_delete_language,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let mut sets_with_meta = Vec::with_capacity(results.items.len());
    for set in &results.items {
        let author = attempt!(s, users.find_by_id(set.created_by).await);
        sets_with_meta.push(SoundChangeSetWithMeta {
            sound_change_set: set.clone(),
            author,
        });
    }

    let template = SearchTemplate {
        current_user,
        language: language_with_contributors,
        error: None,
        previous_query: query,
        previous_pagination: pagination,
        results: Some(results),
        sets_with_meta,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

async fn view(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let set = attempt!(s, sets.get(id).await);
    let Some(set) = set else {
        return render_generic_error(s, crate::err::not_found("Sound change set not found")).await;
    };

    let author = attempt!(s, users.find_by_id(set.created_by).await);

    let (language_with_contributors, can_edit_language, can_delete_language) = attempt!(
        s,
        build_language_context(&s, &languages, &permissions, &contribution_stats, language).await
    );

    let rendered_description = if !set.description.is_empty() {
        attempt!(s, render_md(&set.description).map_err(Into::into))
    } else {
        String::new()
    };

    let template = ViewTemplate {
        current_user: s.user().cloned(),
        language: language_with_contributors,
        set,
        author,
        rendered_description,
        can_edit_language,
        can_delete_language,
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
        .create(
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
            Redirect::to(&format!(
                "/languages/{}/sound-change-sets",
                code
            ))
            .into_response(),
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
                crate::util::will_create_audit_log_for_language(&state, user, set.language_id).await
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
                let language = attempt!(s, languages.find_by_id(set.language_id).await);
                let (lwc, can_edit, _) = attempt!(
                    s,
                    build_language_context(&s, &languages, &permissions, &contribution_stats, language).await
                );
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
                crate::util::will_create_audit_log_for_language(&state, user, set.language_id).await
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
                let language = attempt!(s, languages.find_by_id(set.language_id).await);
                let (lwc, can_edit, _) = attempt!(
                    s,
                    build_language_context(&s, &languages, &permissions, &contribution_stats, language).await
                );
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
    ).await {
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
                traces.into_iter().map(|(input, steps)| {
                    
                    let steps_html: String = steps.iter().enumerate().map(|(i, step)| {
                        let step_input = if i == 0 { &input } else { &steps[i - 1].output };
                        format!("<tr><td>{}</td><td>{step_input}</td><td>{}</td></tr>", step.rule, step.output)
                    }).collect();

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
                }).collect()
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
                error: Some(crate::err::bad_request(format!(
                    "{}",
                    lexurgy_error
                ))),
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
    form: axum::Form<SaveToNewFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);

    let template = SaveToNewTemplate {
        current_user: Some(user),
        error: None,
        available_languages,
        previous_language_code: String::new(),
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
    form: axum::Form<SaveToNewSubmitFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    if form.name.trim().is_empty() || form.language_code.trim().is_empty() {
        let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);
        let template = SaveToNewTemplate {
            current_user: Some(user),
            error: Some(crate::err::bad_request("Language and name are required.")),
            available_languages,
            previous_language_code: form.language_code.clone(),
            previous_name: form.name.clone(),
            previous_description: form.description.clone(),
            changes: form.changes.clone(),
        };
        return (StatusCode::BAD_REQUEST, render_template(template));
    }

    let language = attempt!(s, languages.find_by_code(&form.language_code).await);

    let description = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.clone())
    };

    match sets
        .create(
            &user,
            &language,
            NewSoundChangeSet {
                name: form.name.clone(),
                description,
                changes: form.changes.clone(),
            },
        )
        .await
    {
        Ok(created) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/sound-change-sets/run?set={}", created.id)).into_response(),
        ),
        Err(e) => {
            let available_languages = languages
                .list_editable_by_user(user.id)
                .await
                .unwrap_or_default();
            let template = SaveToNewTemplate {
                current_user: Some(user),
                error: Some(e),
                available_languages,
                previous_language_code: form.language_code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                changes: form.changes.clone(),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}
