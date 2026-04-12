use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use itertools::MultiUnzip;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    err::{AppError, AppResult, forbidden},
    get_user,
    model::{
        definitions::{CreateDefinition, DefinitionRepository, UpdateDefinition},
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::{SoundChangeSet, SoundChangeSetRepository},
        users::User,
        word_classes::{WordClass, WordClassRepository},
        words::{Word, WordRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session, will_create_audit_log_for_language},
};

#[derive(Template)]
#[template(path = "words/edit.html")]
#[allow(dead_code)]
struct EditWordTemplate {
    current_user: Option<User>,
    error: Option<crate::err::AppError>,
    language: crate::model::languages::Language,
    word: Word,
    word_classes: Vec<WordClass>,
    previous_word: String,
    previous_word_class: Option<String>,
    previous_definitions: Vec<String>,
    previous_contexts: Vec<String>,
    previous_definition_ids: Vec<String>,
    previous_ipa: String,
    previous_notes: String,
    user_has_permission: bool,
    will_create_audit_log: bool,
    ipa_estimator: Option<SoundChangeSet>,
}

#[derive(Deserialize)]
pub(super) struct EditWordFormData {
    pub(super) word: String,
    pub(super) word_class: String,
    #[serde(default, rename = "definitions[]")]
    pub(super) definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    pub(super) contexts: Vec<String>,
    #[serde(default, rename = "definition_ids[]")]
    pub(super) definition_ids: Vec<String>,
    pub(super) ipa: Option<String>,
    pub(super) notes: Option<String>,
}

struct EditCommon {
    current_user: User,
    language: Language,
    word: Word,
    word_classes_list: Vec<WordClass>,
    ipa_estimator: Option<SoundChangeSet>,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
}

async fn edit_common(
    s: &Session,
    state: &AppState,
    language_code: &str,
    slug: &str,
    lemma: i32,
) -> AppResult<EditCommon> {
    let languages = LanguageRepository::new(state.clone());
    let permissions = LanguagePermissionRepository::new(state.clone());
    let word_classes = WordClassRepository::new(state.clone());
    let words = WordRepository::new(state.clone());

    let Some(current_user) = s.user().cloned() else {
        return Err(forbidden(""));
    };
    let language = languages.find_by_code(&language_code).await?;
    let word = 
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await?;
    let word_classes_list = word_classes.list_all(language.id).await?;
    let ipa_estimator = languages.get_ipa_estimator(language.id).await?;
    
    let can_edit_language = 
        permissions
            .can_edit_language(Some(&current_user), &language.id)
            .await?;
    let can_delete_language =
        permissions
            .can_delete_language(Some(&current_user), &language.id)
            .await?;
    let will_create_audit_log =
        will_create_audit_log_for_language(&state, &current_user, language.id).await;


    Ok(EditCommon {
        current_user,
        language,
        word,
        word_classes_list,
        ipa_estimator,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    })
}

pub(super) async fn edit_word(
    s: Session,
    State(state): State<AppState>,
    definitions_repo: DefinitionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let EditCommon {
        current_user,
        language,
        word,
        word_classes_list,
        ipa_estimator,
        can_edit_language: user_has_permission,
        will_create_audit_log,
        ..
    } = attempt!(
        s,
        edit_common(
            &s,
            &state,
            &language_code,
            &slug,
            lemma
        )
        .await
    );

    // Fetch existing definitions
    let (previous_definitions, previous_contexts, previous_definition_ids) = attempt!(
        s,
        definitions_repo
            .list_by_word(
                word.id,
                PaginatedRequest::first(100),
            )
            .await
            .map(|res| res
                .items
                .into_iter()
                .map(|d| (d.definition, d.context, d.id.to_string()))
                .multiunzip())
    );

    let template = EditWordTemplate {
        current_user: Some(current_user),
        error: None,
        language,
        word: word.clone(),
        word_classes: word_classes_list,
        previous_word: word.word,
        previous_word_class: word.word_class_abbreviation,
        previous_definitions,
        previous_contexts,
        previous_definition_ids,
        previous_ipa: word.ipa,
        previous_notes: word.notes,
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
    };

    let body = render_template(template);
    okay(body)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn edit_word_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<EditWordFormData>,
) -> (StatusCode, Response) {
    let EditCommon {
        current_user,
        language,
        word,
        word_classes_list,
        ipa_estimator,
        can_edit_language: user_has_permission,
        will_create_audit_log,
        ..
    } = attempt!(
        s,
        edit_common(
            &s,
            &state,
            &language_code,
            &slug,
            lemma
        )
        .await
    );

    let render_err = |error: AppError| {
        let template = EditWordTemplate {
            current_user: Some(user.clone()),
            error: Some(error),
            language: language.clone(),
            word: word.clone(),
            word_classes: word_classes_list,
            previous_word: form.word.clone(),
            previous_word_class: Some(form.word_class.clone()),
            previous_definitions: form.definitions.clone(),
            previous_contexts: form.contexts.clone(),
            previous_definition_ids: form.definition_ids.clone(),
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            user_has_permission,
            will_create_audit_log,
            ipa_estimator,
        };
        let body = render_template(template);
        (StatusCode::BAD_REQUEST, body)
    };

    // Filter out empty definitions and limit to MAX_DEFINITIONS
    let definitions_text: Vec<String> = form
        .definitions
        .iter()
        .filter_map(|d| {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .take(super::MAX_DEFINITIONS)
        .collect();

    // Require at least one definition
    if definitions_text.is_empty() {
        return render_err(crate::err::bad_request("At least one definition is required"));
    }

    // Update the word
    let update_word = crate::model::words::UpdateWord {
        word: Some(form.word.clone()),
        word_class: Some(form.word_class.clone()),
        ipa: form.ipa.clone(),
        notes: form.notes.clone(),
        extra: None,
    };

    let result = async {
        let word_result = words
            .update_by_lemma(&user, language.id, &slug, lemma, update_word)
            .await?;

        // Handle definitions: update existing, create new, delete removed
        let existing_defs = definitions_repo
            .list_by_word(
                word_result.id,
                PaginatedRequest {
                    limit: 100,
                    offset: 0,
                },
            )
            .await?
            .items;

        // Parse definition IDs
        let definition_ids: Vec<Option<Uuid>> = form
            .definition_ids
            .iter()
            .map(|id| {
                if id.is_empty() {
                    None
                } else {
                    id.parse::<Uuid>().ok()
                }
            })
            .collect();

        // Track which existing definitions are being kept
        let mut kept_ids = std::collections::HashSet::new();

        // Update or create definitions
        for (i, def_text) in definitions_text.iter().enumerate() {
            let context = form.contexts.get(i).and_then(|c| {
                let trimmed = c.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

            if let Some(Some(def_id)) = definition_ids.get(i) {
                // Update existing definition
                kept_ids.insert(*def_id);
                let update = UpdateDefinition {
                    definition: Some(def_text.clone()),
                    context: context.clone(),
                };
                definitions_repo
                    .update(&user, *def_id, update, Some(i as i32))
                    .await?;
            } else {
                // Create new definition
                let create_def = CreateDefinition {
                    definition: def_text.clone(),
                    context,
                };
                definitions_repo
                    .create(&user, word_result.id, create_def)
                    .await?;
            }
        }

        // Delete definitions that were removed
        for existing_def in existing_defs {
            if !kept_ids.contains(&existing_def.id) {
                definitions_repo.delete(&user, existing_def.id).await?;
            }
        }

        Ok::<_, crate::err::AppError>(word_result)
    }
    .await;

    match result {
        Ok(word_result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/words/{}/{}",
                language_code, word_result.slug, word_result.lemma
            ))
            .into_response(),
        ),
        Err(e) => render_err(e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn estimate_ipa_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    sets: SoundChangeSetRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<EditWordFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&user), language.id, &slug, lemma)
            .await
    );

    let user_has_permission = attempt!(
        s,
        permissions
            .can_edit_language(Some(&user), &language.id)
            .await
    );

    let will_create_audit_log =
        will_create_audit_log_for_language(&state, &user, language.id).await;

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);
    let ipa_estimator = attempt!(s, languages.get_ipa_estimator(language.id).await);

    let estimated_ipa = match &ipa_estimator {
        Some(scs) => match sets.run_from_db(&scs.id, vec![form.word.clone()]).await {
            Ok(response) => response.output_words.into_iter().next().unwrap_or_default(),
            Err(_) => form.ipa.clone().unwrap_or_default(),
        },
        None => form.ipa.clone().unwrap_or_default(),
    };

    let template = EditWordTemplate {
        current_user: Some(user),
        error: None,
        language,
        word,
        word_classes: word_classes_list,
        previous_word: form.word.clone(),
        previous_word_class: form.word_class.clone(),
        previous_definitions: form.definitions.clone(),
        previous_contexts: form.contexts.clone(),
        previous_definition_ids: form.definition_ids.clone(),
        previous_ipa: estimated_ipa,
        previous_notes: form.notes.clone().unwrap_or_default(),
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
    };

    let body = render_template(template);
    okay(body)
}
