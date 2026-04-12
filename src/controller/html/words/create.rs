use std::str::FromStr as _;

use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    err::{AppError, AppResult, bad_request, field_error, forbidden},
    model::{
        bookmarks::BookmarkRepository,
        definitions::{CreateDefinition, DefinitionRepository},
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::{SoundChangeSet, SoundChangeSetRepository},
        users::User,
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
    previous_word: String,
    previous_word_class: String,
    previous_definition: String,
    previous_definitions: Vec<String>,
    previous_context: String,
    previous_contexts: Vec<String>,
    previous_ipa: String,
    previous_notes: String,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
    ipa_estimator: Option<SoundChangeSet>,
    relation_kind: String,
    antecedent: Option<WordWithMeta>,
}

#[derive(Deserialize, Default)]
pub(super) struct NewWordPrefill {
    word: Option<String>,
    ipa: Option<String>,
    word_class: Option<String>,
    #[serde(default, rename = "definitions[]")]
    definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    contexts: Vec<String>,
    antecedent_bookmark: Option<String>,
    relation_kind: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct NewWordFormData {
    pub(super) word: String,
    pub(super) word_class: String,
    #[serde(default, rename = "definitions[]")]
    pub(super) definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    pub(super) contexts: Vec<String>,
    pub(super) ipa: Option<String>,
    pub(super) notes: Option<String>,
    pub(super) antecedent_bookmark: Option<String>,
    pub(super) relation_kind: Option<String>,
}

fn split_first(v: Vec<String>) -> (String, Vec<String>) {
    let mut iter = v.into_iter();
    let first = iter.next().unwrap_or_default();
    let rest = iter.collect();
    (first, rest)
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
    antecedent_bookmark: Option<&str>
) -> AppResult<CreateCommon> {
    let languages = LanguageRepository::new(state.clone());
    let permissions = LanguagePermissionRepository::new(state.clone());
    let word_classes = WordClassRepository::new(state.clone());

    let Some(current_user) = s.user().cloned() else {
        return Err(forbidden(""));
    };
    let language = languages.find_by_code(language_code).await?;
    let word_classes_list = word_classes.list_all(language.id).await?;
    let ipa_estimator = languages.get_ipa_estimator(language.id).await?;
    let antecedent = lookup_antecedent(state, antecedent_bookmark, &current_user).await;

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

    return Ok(CreateCommon {
        current_user,
        language,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        word_classes_list,
        ipa_estimator
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
        ipa_estimator,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    } = attempt!(s, create_common(&s, &state, &language_code, antecedent_bookmark).await);

    let (previous_definition, previous_definitions) = split_first(prefill.definitions);
    let (previous_context, previous_contexts) = split_first(prefill.contexts);

    let template = NewWordTemplate {
        current_user: Some(current_user),
        error: None,
        language,
        word_classes: word_classes_list,

        previous_word: prefill.word.unwrap_or_default(),
        previous_word_class: prefill.word_class.unwrap_or_default(),
        previous_ipa: prefill.ipa.unwrap_or_default(),
        relation_kind: prefill.relation_kind.unwrap_or_default(),

        previous_definition,
        previous_definitions,
        previous_context,
        previous_contexts,
        previous_notes: String::new(),

        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        ipa_estimator,
        antecedent,
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
    let word = words
        .create_with_tx(user, language_id, create_word, &mut tx)
        .await?;
    for create_definition in create_definitions.into_iter() {
        let _ = definitions_repo
            .create_with_tx(user, word, language_id, create_definition, &mut tx)
            .await?;
    }
    let word = words.find_by_id(word).await?;

    if let Some((source_word, kind)) = word_relation {
        let word_relations = WordRelationRepository::new(state.clone());
        let relation = CreateWordRelation {
            antecedent: source_word.word.clone(),
            consequent: word.clone(),
            kind,
        };
        let _ = word_relations
            .create_with_tx(&user, relation, &mut tx)
            .await;
    }

    tx.commit().await?;

    Ok(word)
}

pub(super) async fn new_word_submit(
    s: Session,
    State(state): State<AppState>,
    Path(language_code): Path<String>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> (StatusCode, Response) {
    let antecedent_bookmark = form.antecedent_bookmark.as_deref();
    let CreateCommon {
        current_user,
        language,
        word_classes_list,
        ipa_estimator,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    } = attempt!(s, create_common(&s, &state, &language_code, antecedent_bookmark).await);

    let relation_kind_str = form.relation_kind.clone().unwrap_or_default();

    // We need to keep a copy of the form definitions and contexts for the `render_err` closure;
    // otherwise, the original `Vec`s get moved into the `render_err` closure and can't be accessed
    // for `create_definitions`.
    let definitions_for_err = form.definitions.clone();
    let contexts_for_err = form.contexts.clone();

    let render_err = |error: AppError| {
        let (previous_definition, previous_definitions) = split_first(definitions_for_err.clone());
        let (previous_context, previous_contexts) = split_first(contexts_for_err.clone());

        let template = NewWordTemplate {
            current_user: Some(current_user.clone()),
            error: Some(error),
            language: language.clone(),
            word_classes: word_classes_list,
            previous_word: form.word.clone(),
            previous_word_class: form.word_class.clone(),
            previous_definition,
            previous_definitions,
            previous_context,
            previous_contexts,
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            can_edit_language,
            can_delete_language,
            will_create_audit_log,
            ipa_estimator,
            relation_kind: relation_kind_str.clone(),
            antecedent: antecedent.clone(),
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
    };

    let word_relation = WordRelationType::from_str(&relation_kind_str)
        .ok()
        .and_then(|kind| antecedent.as_ref().map(|ant| (ant, kind)));

    let result = create_word_and_definitions(
        &current_user,
        language.id,
        create_word,
        create_definitions,
        &state,
        word_relation,
    )
    .await;

    match result {
        Ok(word) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/words/{}/{}",
                language_code, word.slug, word.lemma
            ))
            .into_response(),
        ),
        Err(e) => render_err(e),
    }
}

async fn estimate_ipa(sets: SoundChangeSetRepository, ipa_estimator: &Uuid, word: &str) -> AppResult<String> {
    sets
            .run_from_db(ipa_estimator, vec![word.to_string()])
            .await
            .and_then(|results| {
                if let Some(errors) = results.errors {
                    if errors.is_empty() {
                        Ok(results.output_words.get(0).cloned().unwrap_or_default())
                    } else {
                        Err(bad_request(format!(
                            "IPA estimation failed: {}",
                            errors
                                .into_iter()
                                .map(|e| e.message)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )))
                    }
                } else {
                    Ok(results.output_words.get(0).cloned().unwrap_or_default())
                }
            })
            .map_err(|e| field_error("ipa", e.to_string()))
    
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
        ipa_estimator,
        antecedent,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    } = attempt!(s, create_common(&s, &state, &language_code, antecedent_bookmark).await);

    let (previous_definition, previous_definitions) = split_first(form.definitions);
    let (previous_context, previous_contexts) = split_first(form.contexts);

    let estimated_ipa = if let Some(estimator) = &ipa_estimator {
        match estimate_ipa(sets, &estimator.id, &form.word).await {
            Ok(ipa) => Some(ipa),
            Err(error) => {
                let template = NewWordTemplate {
                    current_user: Some(current_user),
                    error: Some(error),
                    language,
                    word_classes: word_classes_list,

                    previous_word: form.word,
                    previous_word_class: form.word_class,
                    previous_ipa: form.ipa.unwrap_or_default(),
                    relation_kind: form.relation_kind.unwrap_or_default(),

                    previous_definition,
                    previous_definitions,
                    previous_context,
                    previous_contexts,
                    previous_notes: form.notes.unwrap_or_default(),

                    can_edit_language,
                    can_delete_language,
                    will_create_audit_log,
                    ipa_estimator,
                    antecedent,
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
        error: None,
        language,
        word_classes: word_classes_list,

        previous_word: form.word,
        previous_word_class: form.word_class,
        previous_ipa: estimated_ipa.unwrap_or_default(),
        relation_kind: form.relation_kind.unwrap_or_default(),

        previous_definition,
        previous_definitions,
        previous_context,
        previous_contexts,
        previous_notes: form.notes.unwrap_or_default(),

        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        ipa_estimator,
        antecedent,
    };

    let body = render_template(template);
    okay(body)
}
