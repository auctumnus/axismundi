use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::search::build_categories_json;
use crate::{
    attempt,
    controller::html::{okay, render_template, words::estimate_ipa},
    err::{AppError, AppResult, bad_request, forbidden, internal_error},
    model::{
        bookmarks::BookmarkRepository,
        definitions::{CreateDefinition, DefinitionRepository},
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::{SoundChangeSet, SoundChangeSetRepository},
        users::User,
        word_categories::{WordCategory, WordCategoryRepository},
        word_classes::{WordClass, WordClassRepository},
        word_relations::{CreateWordRelation, WordRelationRepository, WordRelationType},
        words::{CreateWord, Word, WordRepository, WordWithMeta},
    },
    util::{AppState, extract_session::Session, will_create_audit_log_for_language},
};

#[derive(Template)]
#[template(path = "words/new.html")]
#[allow(dead_code)]
struct NewWordTemplate {
    current_user: Option<User>,
    error: Option<crate::err::AppError>,
    language: Language,
    word_classes: Vec<WordClass>,
    word_categories: Vec<WordCategory>,
    selected_category_abbrevs: Vec<String>,
    word_categories_json: String,
    previous_word: String,
    previous_word_class: String,
    previous_definition: String,
    previous_definitions: Vec<String>,
    previous_context: String,
    previous_contexts: Vec<String>,
    previous_definitions_json: String,
    previous_ipa: String,
    previous_notes: String,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
    ipa_estimator: Option<SoundChangeSet>,
    derived_from: Option<(WordWithMeta, WordRelationType)>,
    nojs_slots: Vec<(String, String)>,
}

fn nojs_slots_new(
    first_def: &str,
    first_ctx: &str,
    rest_defs: &[String],
    rest_ctxs: &[String],
) -> Vec<(String, String)> {
    let mut slots: Vec<(String, String)> = Vec::with_capacity(5);
    slots.push((first_def.to_string(), first_ctx.to_string()));
    for (i, def) in rest_defs.iter().enumerate().take(4) {
        let ctx = rest_ctxs
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        slots.push((def.clone(), ctx));
    }
    while slots.len() < 5 {
        slots.push((String::new(), String::new()));
    }
    slots
}

#[derive(Deserialize, Default, Serialize)]
pub(super) struct NewWordPrefill {
    pub word: Option<String>,
    pub ipa: Option<String>,
    pub word_class: Option<String>,
    #[serde(default, rename = "definitions[]")]
    pub definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    pub contexts: Vec<String>,
    #[serde(default, rename = "categories[]")]
    pub categories: Vec<String>,
    pub antecedent_bookmark: Option<String>,
    pub relation_kind: Option<WordRelationType>,
}

#[derive(Deserialize, Default)]
pub(super) struct NewWordSubmitQuery {
    #[serde(rename = "continue")]
    pub continue_: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct NewWordFormData {
    pub(super) word: String,
    pub(super) word_class: String,
    #[serde(default, rename = "definitions[]")]
    pub(super) definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    pub(super) contexts: Vec<String>,
    #[serde(default, rename = "categories[]")]
    pub(super) categories: Vec<String>,
    pub(super) ipa: Option<String>,
    pub(super) notes: Option<String>,
    pub(super) antecedent_bookmark: Option<String>,
    pub(super) relation_kind: Option<WordRelationType>,
}

fn split_first(v: Vec<String>) -> (String, Vec<String>) {
    let mut iter = v.into_iter();
    let first = iter.next().unwrap_or_default();
    let rest = iter.collect();
    (first, rest)
}

fn build_definitions_json(defs: &[String], ctxs: &[String]) -> String {
    let items: Vec<serde_json::Value> = defs
        .iter()
        .enumerate()
        .map(|(i, def)| {
            let ctx = ctxs.get(i).map(|s| s.as_str()).unwrap_or("");
            serde_json::json!({"id": "", "definition": def, "context": ctx})
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

async fn lookup_antecedent(
    state: &AppState,
    antecedent_bookmark: Option<&str>,
    user: &User,
) -> Option<WordWithMeta> {
    let bookmarks = BookmarkRepository::new(state.clone());
    let words = WordRepository::new(state.clone());
    let antecedent_bookmark = antecedent_bookmark?;
    if antecedent_bookmark.is_empty() {
        return None;
    }
    let bookmark = bookmarks.get_by_slug(antecedent_bookmark).await.ok()?;
    let word = words.find_by_id(bookmark.item).await.ok()?;
    words.materialize(word, Some(user)).await.ok()
}

struct CreateCommon {
    current_user: User,
    language: Language,
    word_classes_list: Vec<WordClass>,
    word_categories_list: Vec<WordCategory>,
    ipa_estimator: Option<SoundChangeSet>,
    antecedent: Option<WordWithMeta>,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
}

/// Shared logic for the different routes.
async fn create_common(
    s: &Session,
    state: &AppState,
    language_code: &str,
    antecedent_bookmark: Option<&str>,
) -> AppResult<CreateCommon> {
    let languages = LanguageRepository::new(state.clone());
    let permissions = LanguagePermissionRepository::new(state.clone());
    let word_classes = WordClassRepository::new(state.clone());
    let word_categories = WordCategoryRepository::new(state.clone());

    let Some(current_user) = s.user().cloned() else {
        return Err(forbidden(""));
    };
    let language = languages.find_by_code(language_code).await?;
    let word_classes_list = word_classes.list_all(language.id).await?;
    let word_categories_list = word_categories.list_all(language.id).await?;
    let ipa_estimator = languages.get_ipa_estimator(language.id).await?;
    let antecedent = lookup_antecedent(state, antecedent_bookmark, &current_user).await;

    let can_edit_language = permissions
        .can_edit_language(Some(&current_user), &language.id)
        .await?;
    let can_delete_language = permissions
        .can_delete_language(Some(&current_user), &language.id)
        .await?;
    let will_create_audit_log =
        will_create_audit_log_for_language(&state, &current_user, language.id).await;

    return Ok(CreateCommon {
        current_user,
        language,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        word_classes_list,
        word_categories_list,
        ipa_estimator,
    });
}

pub(super) async fn new_word(
    s: Session,
    State(state): State<AppState>,
    Path(language_code): Path<String>,
    axum_extra::extract::Query(prefill): axum_extra::extract::Query<NewWordPrefill>,
) -> (StatusCode, Response) {
    let antecedent_bookmark = prefill.antecedent_bookmark.as_deref();
    let CreateCommon {
        current_user,
        language,
        word_classes_list,
        word_categories_list,
        ipa_estimator,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    } = attempt!(
        s,
        create_common(&s, &state, &language_code, antecedent_bookmark).await
    );

    let previous_definitions_json = build_definitions_json(&prefill.definitions, &prefill.contexts);
    let (previous_definition, previous_definitions) = split_first(prefill.definitions);
    let (previous_context, previous_contexts) = split_first(prefill.contexts);
    let nojs_slots = nojs_slots_new(
        &previous_definition,
        &previous_context,
        &previous_definitions,
        &previous_contexts,
    );

    let (error, derived_from) = match antecedent {
        None => (None, None),
        Some(ant) => match prefill.relation_kind {
            Some(rk) => (None, Some((ant, rk))),
            None => (
                Some(internal_error(
                    "no relation kind was found in the word prefill; please report!",
                )),
                None,
            ),
        },
    };

    let word_categories_json = build_categories_json(&word_categories_list);

    let template = NewWordTemplate {
        current_user: Some(current_user),
        error,
        language,
        word_classes: word_classes_list,
        word_categories: word_categories_list,
        selected_category_abbrevs: prefill.categories,
        word_categories_json,

        previous_word: prefill.word.unwrap_or_default(),
        previous_word_class: prefill.word_class.unwrap_or_default(),
        previous_ipa: prefill.ipa.unwrap_or_default(),

        previous_definition,
        previous_definitions,
        previous_context,
        previous_contexts,
        previous_definitions_json,
        previous_notes: String::new(),

        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        ipa_estimator,
        derived_from,
        nojs_slots,
    };

    let body = render_template(template);
    okay(body)
}

async fn create_word_and_definitions(
    user: &User,
    language_id: Uuid,
    create_word: CreateWord,
    create_definitions: Vec<CreateDefinition>,
    state: &AppState,
    word_relation: Option<(&WordWithMeta, WordRelationType)>,
) -> Result<Word, crate::err::AppError> {
    let words = WordRepository::new(state.clone());
    let definitions_repo = DefinitionRepository::new(state.clone());
    let mut tx = state.pool.begin().await?;
    let word_id = words
        .create_with_tx(user, language_id, create_word, &mut tx)
        .await?;
    for create_definition in create_definitions.into_iter() {
        let _ = definitions_repo
            .create_with_tx(user, word_id, language_id, create_definition, &mut tx)
            .await?;
    }

    if let Some((source_word, kind)) = word_relation {
        let word_relations = WordRelationRepository::new(state.clone());
        let relation = CreateWordRelation {
            antecedent: source_word.word.id,
            consequent: word_id,
            kind,
        };
        let _ = word_relations
            .create_with_tx(&user, relation, &mut tx)
            .await;
    }

    tx.commit().await?;

    let word = words.find_by_id(word_id).await?;
    Ok(word)
}

pub(super) async fn new_word_submit(
    s: Session,
    State(state): State<AppState>,
    Path(language_code): Path<String>,
    axum::extract::Query(query): axum::extract::Query<NewWordSubmitQuery>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> (StatusCode, Response) {
    let antecedent_bookmark = form.antecedent_bookmark.as_deref();
    let CreateCommon {
        current_user,
        language,
        word_classes_list,
        word_categories_list,
        ipa_estimator,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    } = attempt!(
        s,
        create_common(&s, &state, &language_code, antecedent_bookmark).await
    );

    let (_, derived_from) = match antecedent {
        None => (None, None),
        Some(ant) => match form.relation_kind {
            Some(rk) => (None, Some((ant, rk))),
            None => (
                Some(internal_error(
                    "no relation kind was found in the word prefill; please report!",
                )),
                None,
            ),
        },
    };

    // We need to keep a copy of the form definitions and contexts for the `render_err` closure;
    // otherwise, the original `Vec`s get moved into the `render_err` closure and can't be accessed
    // for `create_definitions`.
    let definitions_for_err = form.definitions.clone();
    let contexts_for_err = form.contexts.clone();
    let categories_for_err = form.categories.clone();

    let render_err = |error: AppError| {
        let previous_definitions_json =
            build_definitions_json(&definitions_for_err, &contexts_for_err);
        let (previous_definition, previous_definitions) = split_first(definitions_for_err.clone());
        let (previous_context, previous_contexts) = split_first(contexts_for_err.clone());
        let nojs_slots = nojs_slots_new(
            &previous_definition,
            &previous_context,
            &previous_definitions,
            &previous_contexts,
        );

        let word_categories_json = build_categories_json(&word_categories_list);

        let template = NewWordTemplate {
            current_user: Some(current_user.clone()),
            error: Some(error),
            language: language.clone(),
            word_classes: word_classes_list,
            word_categories: word_categories_list,
            selected_category_abbrevs: categories_for_err.clone(),
            word_categories_json,
            previous_word: form.word.clone(),
            previous_word_class: form.word_class.clone(),
            previous_definition,
            previous_definitions,
            previous_context,
            previous_contexts,
            previous_definitions_json,
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            can_edit_language,
            can_delete_language,
            will_create_audit_log,
            ipa_estimator,
            derived_from: derived_from.clone(),
            nojs_slots,
        };

        let body = render_template(template);
        return (StatusCode::BAD_REQUEST, body);
    };

    // Filter out empty definitions and limit to 10
    let create_definitions: AppResult<Vec<CreateDefinition>> = form
        .definitions
        .into_iter()
        .zip(
            form.contexts
                .into_iter()
                .chain(std::iter::repeat(String::new())),
        )
        .filter_map(|(def, ctx)| {
            let definition = def.trim();
            let context = ctx.trim();
            if definition.is_empty() {
                if context.is_empty() {
                    None
                } else {
                    Some(Err(bad_request(
                        "Definition cannot be empty if context is provided",
                    )))
                }
            } else {
                let definition = definition.to_string();
                let context = if context.is_empty() {
                    None
                } else {
                    Some(context.to_string())
                };
                Some(Ok(CreateDefinition {
                    definition,
                    context,
                }))
            }
        })
        .take(super::MAX_DEFINITIONS)
        .collect();

    let create_definitions = match create_definitions {
        Ok(defs) => defs,
        Err(e) => return render_err(e),
    };

    // Require at least one definition
    if create_definitions.is_empty() {
        return render_err(bad_request("At least one definition is required"));
    }

    let create_word = CreateWord {
        word: form.word.clone(),
        word_class: form.word_class.clone(),
        ipa: form.ipa.clone(),
        notes: form.notes.clone(),
        extra: None,
        categories: Some(form.categories.clone()),
        definitions: None,
    };

    let result = create_word_and_definitions(
        &current_user,
        language.id,
        create_word,
        create_definitions,
        &state,
        derived_from.as_ref().map(|(w, rk)| (w, *rk)),
    )
    .await;

    match result {
        Ok(word) => {
            let redirect_url = if query.continue_.is_some() {
                format!("/languages/{}/new-word", language_code)
            } else {
                format!(
                    "/languages/{}/words/{}/{}",
                    language_code, word.slug, word.lemma
                )
            };
            (
                StatusCode::SEE_OTHER,
                Redirect::to(&redirect_url).into_response(),
            )
        }
        Err(e) => render_err(e),
    }
}

pub(super) async fn estimate_ipa_new_word(
    s: Session,
    State(state): State<AppState>,
    sets: SoundChangeSetRepository,
    Path(language_code): Path<String>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> (StatusCode, Response) {
    let antecedent_bookmark = form.antecedent_bookmark.as_deref();
    let CreateCommon {
        current_user,
        language,
        word_classes_list,
        word_categories_list,
        ipa_estimator,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    } = attempt!(
        s,
        create_common(&s, &state, &language_code, antecedent_bookmark).await
    );

    let (error, derived_from) = match antecedent {
        None => (None, None),
        Some(ant) => match form.relation_kind {
            Some(rk) => (None, Some((ant, rk))),
            None => (
                Some(internal_error(
                    "no relation kind was found in the word prefill; please report!",
                )),
                None,
            ),
        },
    };

    let previous_definitions_json = build_definitions_json(&form.definitions, &form.contexts);
    let (previous_definition, previous_definitions) = split_first(form.definitions);
    let (previous_context, previous_contexts) = split_first(form.contexts);
    let nojs_slots = nojs_slots_new(
        &previous_definition,
        &previous_context,
        &previous_definitions,
        &previous_contexts,
    );

    let selected_category_abbrevs = form.categories.clone();
    let word_categories_json = build_categories_json(&word_categories_list);

    let estimated_ipa = if let Some(estimator) = &ipa_estimator {
        match estimate_ipa(sets, &estimator.id, &form.word).await {
            Ok(ipa) => Some(ipa),
            Err(error) => {
                let template = NewWordTemplate {
                    current_user: Some(current_user),
                    error: Some(error),
                    language,
                    word_classes: word_classes_list,
                    word_categories: word_categories_list,
                    selected_category_abbrevs,
                    word_categories_json,

                    previous_word: form.word,
                    previous_word_class: form.word_class,
                    previous_ipa: form.ipa.unwrap_or_default(),

                    previous_definition,
                    previous_definitions,
                    previous_context,
                    previous_contexts,
                    previous_definitions_json: previous_definitions_json.clone(),
                    previous_notes: form.notes.unwrap_or_default(),

                    can_edit_language,
                    can_delete_language,
                    will_create_audit_log,
                    ipa_estimator,
                    derived_from,
                    nojs_slots,
                };

                let body = render_template(template);
                return (StatusCode::BAD_REQUEST, body);
            }
        }
    } else {
        None
    };

    let template = NewWordTemplate {
        current_user: Some(current_user),
        error,
        language,
        word_classes: word_classes_list,
        word_categories: word_categories_list,
        selected_category_abbrevs,
        word_categories_json,

        previous_word: form.word,
        previous_word_class: form.word_class,
        previous_ipa: estimated_ipa.unwrap_or_default(),

        previous_definition,
        previous_definitions,
        previous_context,
        previous_contexts,
        previous_definitions_json,
        previous_notes: form.notes.unwrap_or_default(),

        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        ipa_estimator,
        derived_from,
        nojs_slots,
    };

    let body = render_template(template);
    okay(body)
}
