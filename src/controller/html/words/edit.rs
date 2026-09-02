use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use itertools::MultiUnzip;
use serde::Deserialize;
use uuid::Uuid;

use super::search::build_categories_json;
use crate::{
    attempt,
    controller::html::{okay, render_template, words::estimate_ipa},
    err::{AppError, AppResult, forbidden},
    get_user,
    model::{
        definitions::{CreateDefinition, DefinitionRepository, UpdateDefinition},
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::{SoundChangeSet, SoundChangeSetRepository},
        users::User,
        word_categories::{WordCategory, WordCategoryRepository},
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
    word_categories: Vec<WordCategory>,
    selected_category_abbrevs: Vec<String>,
    word_categories_json: String,
    previous_word: String,
    previous_word_class: Option<String>,
    previous_definitions: Vec<String>,
    previous_contexts: Vec<String>,
    previous_definition_ids: Vec<String>,
    definitions_json: String,
    previous_ipa: String,
    previous_notes: String,
    user_has_permission: bool,
    will_create_audit_log: bool,
    ipa_estimator: Option<SoundChangeSet>,
    nojs_slots: Vec<(String, String, String)>,
}

fn nojs_slots_edit(
    defs: &[String],
    ctxs: &[String],
    ids: &[String],
) -> Vec<(String, String, String)> {
    let mut slots: Vec<(String, String, String)> = (0..defs.len())
        .map(|i| {
            (
                defs[i].clone(),
                ctxs.get(i).cloned().unwrap_or_default(),
                ids.get(i).cloned().unwrap_or_default(),
            )
        })
        .take(5)
        .collect();
    while slots.len() < 5 {
        slots.push((String::new(), String::new(), String::new()));
    }
    slots
}

fn build_definitions_json(defs: &[String], ctxs: &[String], ids: &[String]) -> String {
    let items: Vec<serde_json::Value> = defs
        .iter()
        .enumerate()
        .map(|(i, def)| {
            let ctx = ctxs.get(i).map(|s| s.as_str()).unwrap_or("");
            let id = ids.get(i).map(|s| s.as_str()).unwrap_or("");
            serde_json::json!({"id": id, "definition": def, "context": ctx})
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
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
    #[serde(default, rename = "categories[]")]
    pub(super) categories: Vec<String>,
    #[serde(default)]
    pub(super) ipa: String,
    #[serde(default)]
    pub(super) notes: String,
}

#[allow(dead_code)]
struct EditCommon {
    current_user: User,
    language: Language,
    word: Word,
    word_classes_list: Vec<WordClass>,
    word_categories_list: Vec<WordCategory>,
    current_category_abbrevs: Vec<String>,
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
    let word_categories = WordCategoryRepository::new(state.clone());
    let words = WordRepository::new(state.clone());

    let Some(current_user) = s.user().cloned() else {
        return Err(forbidden(""));
    };
    let language = languages.find_by_code(&language_code).await?;
    let word = words
        .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
        .await?;
    let word_classes_list = word_classes.list_all(language.id).await?;
    let word_categories_list = word_categories.list_all(language.id).await?;
    let current_category_abbrevs = word_categories
        .list_by_word(word.id, None)
        .await?
        .into_iter()
        .map(|c| c.abbreviation)
        .collect();
    let ipa_estimator = languages.get_ipa_estimator(language.id).await?;

    let can_edit_language = permissions
        .can_edit_language(Some(&current_user), &language.id)
        .await?;
    let can_delete_language = permissions
        .can_delete_language(Some(&current_user), &language.id)
        .await?;
    let will_create_audit_log =
        will_create_audit_log_for_language(&state, &current_user, language.id).await;

    Ok(EditCommon {
        current_user,
        language,
        word,
        word_classes_list,
        word_categories_list,
        current_category_abbrevs,
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
        word_categories_list,
        current_category_abbrevs,
        ipa_estimator,
        can_edit_language: user_has_permission,
        will_create_audit_log,
        ..
    } = attempt!(
        s,
        edit_common(&s, &state, &language_code, &slug, lemma).await
    );

    // Fetch existing definitions
    let (previous_definitions, previous_contexts, previous_definition_ids): (
        Vec<String>,
        Vec<String>,
        Vec<String>,
    ) = attempt!(
        s,
        definitions_repo
            .list_by_word(word.id, PaginatedRequest::first(100),)
            .await
            .map(|res| res
                .items
                .into_iter()
                .map(|d| (d.definition, d.context, d.id.to_string()))
                .multiunzip())
    );

    let definitions_json = build_definitions_json(
        &previous_definitions,
        &previous_contexts,
        &previous_definition_ids,
    );
    let nojs_slots = nojs_slots_edit(
        &previous_definitions,
        &previous_contexts,
        &previous_definition_ids,
    );

    let word_categories_json = build_categories_json(&word_categories_list);

    let template = EditWordTemplate {
        current_user: Some(current_user),
        error: None,
        language,
        word: word.clone(),
        word_classes: word_classes_list,
        word_categories: word_categories_list,
        selected_category_abbrevs: current_category_abbrevs,
        word_categories_json,
        previous_word: word.word,
        previous_word_class: word.word_class_abbreviation,
        previous_definitions,
        previous_contexts,
        previous_definition_ids,
        definitions_json,
        previous_ipa: word.ipa,
        previous_notes: word.notes,
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
        nojs_slots,
    };

    let body = render_template(template);
    okay(body)
}

pub(super) async fn edit_word_submit(
    s: Session,
    State(state): State<AppState>,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<EditWordFormData>,
) -> (StatusCode, Response) {
    let EditCommon {
        current_user,
        language,
        word,
        word_classes_list,
        word_categories_list,
        ipa_estimator,
        can_edit_language: user_has_permission,
        will_create_audit_log,
        ..
    } = attempt!(
        s,
        edit_common(&s, &state, &language_code, &slug, lemma).await
    );

    let render_err = |error: AppError| {
        let definitions_json =
            build_definitions_json(&form.definitions, &form.contexts, &form.definition_ids);
        let nojs_slots = nojs_slots_edit(&form.definitions, &form.contexts, &form.definition_ids);
        let word_categories_json = build_categories_json(&word_categories_list);
        let template = EditWordTemplate {
            current_user: Some(current_user.clone()),
            error: Some(error),
            language: language.clone(),
            word: word.clone(),
            word_classes: word_classes_list,
            word_categories: word_categories_list,
            selected_category_abbrevs: form.categories.clone(),
            word_categories_json,
            previous_word: form.word.clone(),
            previous_word_class: Some(form.word_class.clone()),
            previous_definitions: form.definitions.clone(),
            previous_contexts: form.contexts.clone(),
            previous_definition_ids: form.definition_ids.clone(),
            definitions_json,
            previous_ipa: form.ipa.clone(),
            previous_notes: form.notes.clone(),
            user_has_permission,
            will_create_audit_log,
            ipa_estimator,
            nojs_slots,
        };
        let body = render_template(template);
        (StatusCode::BAD_REQUEST, body)
    };

    // Keep every field for a definition together. The editor submits each
    // definition, context, and ID in display order, and filtering just the
    // definitions would desynchronise those parallel lists.
    let submitted_definitions: Vec<(String, String, Option<Uuid>)> = form
        .definitions
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some((
                    trimmed.to_string(),
                    form.contexts
                        .get(i)
                        .map(|c| c.trim().to_string())
                        .unwrap_or_default(),
                    form.definition_ids.get(i).and_then(|id| {
                        if id.is_empty() {
                            None
                        } else {
                            id.parse::<Uuid>().ok()
                        }
                    }),
                ))
            }
        })
        .take(super::MAX_DEFINITIONS)
        .collect();

    // Require at least one definition
    if submitted_definitions.is_empty() {
        return render_err(crate::err::bad_request(
            "At least one definition is required",
        ));
    }

    // Update the word
    let update_word = crate::model::words::UpdateWord {
        word: Some(form.word.clone()),
        word_class: Some(form.word_class.clone()),
        ipa: Some(form.ipa.clone()),
        notes: Some(form.notes.clone()),
        extra: None,
        categories: Some(form.categories.clone()),
    };

    let result = async {
        let word_result = words
            .update_by_lemma(&current_user, language.id, &slug, lemma, update_word)
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

        // Track which existing definitions are being kept
        let mut kept_ids = std::collections::HashSet::new();
        let mut ordered_ids = Vec::with_capacity(submitted_definitions.len());

        // Update or create definitions, then assign positions together below.
        // A new definition is created at the end by default, so assigning its
        // final position here is what preserves a move made in the same edit.
        for (def_text, context, definition_id) in &submitted_definitions {
            if let Some(def_id) = definition_id {
                // Update existing definition
                kept_ids.insert(*def_id);
                let update = UpdateDefinition {
                    definition: Some(def_text.clone()),
                    context: Some(context.clone()),
                };
                definitions_repo
                    .update(&current_user, *def_id, update, None)
                    .await?;
                ordered_ids.push(*def_id);
            } else {
                // Create new definition
                let create_def = CreateDefinition {
                    definition: def_text.clone(),
                    context: Some(context.clone()),
                };
                let created = definitions_repo
                    .create(&current_user, word_result.id, create_def)
                    .await?;
                ordered_ids.push(created.id);
            }
        }

        // Delete definitions that were removed
        for existing_def in existing_defs {
            if !kept_ids.contains(&existing_def.id) {
                definitions_repo
                    .delete(&current_user, existing_def.id)
                    .await?;
            }
        }

        definitions_repo
            .set_positions(word_result.id, &ordered_ids)
            .await?;

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
    let word_categories_repo =
        crate::model::word_categories::WordCategoryRepository::new(state.clone());
    let word_categories_list = attempt!(s, word_categories_repo.list_all(language.id).await);
    let ipa_estimator = attempt!(s, languages.get_ipa_estimator(language.id).await);

    let definitions_json =
        build_definitions_json(&form.definitions, &form.contexts, &form.definition_ids);
    let nojs_slots = nojs_slots_edit(&form.definitions, &form.contexts, &form.definition_ids);
    let word_categories_json = build_categories_json(&word_categories_list);

    let estimated_ipa = match &ipa_estimator {
        Some(scs) => match estimate_ipa(
            sets,
            &scs.id,
            &form.word,
            &crate::placeholders::Placeholders::default()
                .with_ipa(Some(&form.ipa))
                .with_extra(word.extra.as_ref()),
        )
        .await
        {
            Ok(response) => response,
            Err(error) => {
                let status = error.status_code;
                let template = EditWordTemplate {
                    current_user: Some(user),
                    error: Some(error),
                    language,
                    word,
                    word_classes: word_classes_list,
                    word_categories: word_categories_list,
                    selected_category_abbrevs: form.categories.clone(),
                    word_categories_json,
                    previous_word: form.word.clone(),
                    previous_word_class: Some(form.word_class.clone()),
                    previous_definitions: form.definitions.clone(),
                    previous_contexts: form.contexts.clone(),
                    previous_definition_ids: form.definition_ids.clone(),
                    definitions_json,
                    previous_ipa: form.ipa.clone(),
                    previous_notes: form.notes.clone(),
                    user_has_permission,
                    will_create_audit_log,
                    ipa_estimator,
                    nojs_slots,
                };

                let body = render_template(template);
                return (status, body);
            }
        },
        None => form.ipa.clone(),
    };

    let template = EditWordTemplate {
        current_user: Some(user),
        error: None,
        language,
        word,
        word_classes: word_classes_list,
        word_categories: word_categories_list,
        selected_category_abbrevs: form.categories.clone(),
        word_categories_json,
        previous_word: form.word.clone(),
        previous_word_class: Some(form.word_class.clone()),
        previous_definitions: form.definitions.clone(),
        previous_contexts: form.contexts.clone(),
        previous_definition_ids: form.definition_ids.clone(),
        definitions_json,
        previous_ipa: estimated_ipa,
        previous_notes: form.notes.clone(),
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
        nojs_slots,
    };

    let body = render_template(template);
    okay(body)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::{
        controller::api::tests::{
            create_test_language, create_test_word, get, make_authed_user, post,
        },
        email::MockEmailService,
    };

    #[tokio::test]
    async fn new_definition_keeps_its_position_when_reordered_before_saving() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();
        let token = make_authed_user(&crate::tests::random_name(), &app, email_service).await;

        let language = create_test_language(&token, &mut app).await;
        let code = language["code"].as_str().unwrap();
        let word = create_test_word(&token, &mut app, code).await;
        let slug = word["slug"].as_str().unwrap();
        let lemma = word["lemma"].as_i64().unwrap();

        let mut original_ids = Vec::new();
        for definition in ["first definition", "second definition"] {
            let response = app
                .call(
                    post(
                        &token,
                        &format!("languages/{code}/words/{slug}/{lemma}/definitions"),
                        json!({ "definition": definition }),
                    )
                    .await,
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            original_ids.push(
                crate::tests::response_to_value(response.into_body()).await["id"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
        }

        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("word", word["word"].as_str().unwrap());
        form.append_pair("word_class", "n");
        for (id, definition) in [
            ("", "new definition"),
            (original_ids[0].as_str(), "first definition"),
            (original_ids[1].as_str(), "second definition"),
        ] {
            form.append_pair("definition_ids[]", id);
            form.append_pair("definitions[]", definition);
            form.append_pair("contexts[]", "");
        }

        let response = app
            .call(
                Request::builder()
                    .uri(format!("/languages/{code}/words/{slug}/{lemma}/edit"))
                    .method("POST")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form.finish()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let response = app
            .call(
                get(&format!(
                    "languages/{code}/words/{slug}/{lemma}/definitions"
                ))
                .await,
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        let definitions = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|definition| definition["definition"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            definitions,
            ["new definition", "first definition", "second definition"]
        );
    }
}
