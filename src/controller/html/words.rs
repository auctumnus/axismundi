use askama::Template;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Router,
};
use serde::Deserialize;

use crate::{
    controller::html::{render_template, error_template},
    err::{AppError, not_found},
    model::{
        definitions::{Definition, DefinitionRepository, CreateDefinition, UpdateDefinition},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_classes::{WordClass, WordClassRepository},
        words::{Word, WordRepository, WordSearch, CreateWord},
    },
    pagination::PaginatedRequest,
    util::{extract_session::Session, AppState},
};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "words/search.html")]
#[allow(dead_code)]
struct WordSearchTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    previous_query: String,
    results: Option<Vec<Word>>,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/lemmata.html")]
#[allow(dead_code)]
struct LemmataTemplate {
    current_user: Option<User>,
    language: Language,
    word: String,
    lemmata: Vec<Word>,
    parts_of_speech: Vec<String>,
    words_definitions: Vec<Vec<Definition>>,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/lemma.html")]
#[allow(dead_code)]
struct LemmaTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    definitions: Vec<Definition>,
    other_lemmata: bool,
    previous_search: String,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/new.html")]
#[allow(dead_code)]
struct NewWordTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
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
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/new_definition.html")]
#[allow(dead_code)]
struct NewDefinitionTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    word: Word,
    previous_definition: String,
    previous_context: String,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/edit_definition.html")]
#[allow(dead_code)]
struct EditDefinitionTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    word: Word,
    definition: Definition,
    previous_definition: String,
    previous_context: String,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/delete_definition.html")]
#[allow(dead_code)]
struct DeleteDefinitionTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    word: Word,
    definition: Definition,
    user_has_permission: bool,
}

#[derive(Deserialize)]
struct WordSearchQuery {
    #[serde(default)]
    q: String,
}

#[derive(Deserialize)]
struct NewWordFormData {
    word: String,
    word_class: String,
    #[serde(default, rename = "definitions[]")]
    definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    contexts: Vec<String>,
    ipa: Option<String>,
    notes: Option<String>,
}

#[derive(Deserialize)]
struct DefinitionFormData {
    definition: String,
    context: Option<String>,
}

async fn word_search(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    Query(query): Query<WordSearchQuery>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let search = if !query.q.is_empty() {
        WordSearch {
            q: Some(query.q.clone()),
            ..Default::default()
        }
    } else {
        Default::default()
    };

    let results = match words
            .search(language.id, PaginatedRequest::default(), search)
            .await
        {
            Ok(res) => Some(res.items),
            Err(e) => {
                let template = WordSearchTemplate {
                    current_user,
                    error: Some(e),
                    language,
                    previous_query: query.q,
                    results: None,
                    user_has_permission,
                };
                let body = render_template(template);
                return (StatusCode::BAD_REQUEST, body).into_response();
            }
        };

    let template = WordSearchTemplate {
        current_user,
        error: None,
        language,
        previous_query: query.q,
        results,
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn view_lemmata(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    // Search for all words with this slug
    let search = WordSearch {
        exact_slug: Some(slug.clone()),
        ..Default::default()
    };

    let lemmata = match words
        .search(language.id, PaginatedRequest::default(), search)
        .await
    {
        Ok(res) => res.items,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    if lemmata.is_empty() {
        let body = error_template(current_user.as_ref())(not_found(format!(
            "word with slug '{slug}'"
        )));
        return (StatusCode::NOT_FOUND, body).into_response();
    }

    // If there's only one lemma, redirect to it directly
    if lemmata.len() == 1 {
        let lemma = &lemmata[0];
        return Redirect::to(&format!(
            "/languages/{}/words/{}/{}",
            language_code, slug, lemma.lemma
        ))
        .into_response();
    }

    let word = lemmata[0].word.clone();

    // Fetch the word class names and definitions for each lemma
    let mut parts_of_speech = Vec::new();
    let mut words_definitions = Vec::new();

    for lemma in &lemmata {
        let pos_name = if let Some(word_class_id) = lemma.word_class {
            match word_classes.find_by_id(word_class_id).await {
                Ok(wc) => wc.name,
                Err(_) => "Unknown".to_string(),
            }
        } else {
            "Unknown".to_string()
        };
        parts_of_speech.push(pos_name);

        // Fetch top 10 definitions for this lemma
        let definitions = match definitions_repo
            .list_by_word(lemma.id, PaginatedRequest { limit: 10, offset: 0 })
            .await
        {
            Ok(res) => res.items,
            Err(_) => vec![],
        };
        words_definitions.push(definitions);

        println!("Lemma ID: {}, Definitions: {:?}", lemma.id, words_definitions.last());
    }

    let template = LemmataTemplate {
        current_user,
        language,
        word,
        lemmata,
        parts_of_speech,
        words_definitions,
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn new_word(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let word_classes_list = match word_classes.list_all(language.id).await {
        Ok(classes) => classes,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::INTERNAL_SERVER_ERROR, body).into_response();
        }
    };

    let template = NewWordTemplate {
        current_user,
        error: None,
        language,
        word_classes: word_classes_list,
        previous_word: String::new(),
        previous_word_class: String::new(),
        previous_definition: String::new(),
        previous_definitions: Vec::new(),
        previous_context: String::new(),
        previous_contexts: Vec::new(),
        previous_ipa: String::new(),
        previous_notes: String::new(),
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn new_word_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> impl IntoResponse {
    let Some(user) = s.user().cloned() else {
        return Redirect::to("/login").into_response();
    };

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let word_classes_list = match word_classes.list_all(language.id).await {
        Ok(classes) => classes,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::INTERNAL_SERVER_ERROR, body).into_response();
        }
    };

    // Filter out empty definitions and limit to 10
    const MAX_DEFINITIONS: usize = 10;
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
        .take(MAX_DEFINITIONS)
        .collect();

    // Require at least one definition
    if definitions_text.is_empty() {
        let template = NewWordTemplate {
            current_user: Some(user),
            error: Some(crate::err::bad_request("At least one definition is required")),
            language,
            word_classes: word_classes_list,
            previous_word: form.word.clone(),
            previous_word_class: form.word_class.clone(),
            previous_definition: form.definitions.first().map(|s| s.clone()).unwrap_or_default(),
            previous_definitions: form.definitions.iter().skip(1).map(|s| s.clone()).collect(),
            previous_context: form.contexts.first().map(|s| s.clone()).unwrap_or_default(),
            previous_contexts: form.contexts.iter().skip(1).map(|s| s.clone()).collect(),
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            user_has_permission,
        };

        let body = render_template(template);
        return (StatusCode::BAD_REQUEST, body).into_response();
    }

    let create_word = CreateWord {
        word: form.word.clone(),
        word_class: form.word_class.clone(),
        ipa: form.ipa.clone(),
        notes: form.notes.clone(),
        extra: None,
    };

    // Use a transaction to create word and all definitions atomically
    let result = async {
        let word = words.create(&user, language.id, create_word).await?;

        // Create all definitions in the definitions table
        for (i, def_text) in definitions_text.iter().enumerate() {
            // Get the corresponding context if it exists and is not empty
            let context = form.contexts.get(i).and_then(|c| {
                let trimmed = c.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

            let create_def = CreateDefinition {
                definition: def_text.clone(),
                context,
            };

            definitions_repo.create(&user, word.id, create_def).await?;
        }

        Ok::<_, crate::err::AppError>(word)
    }
    .await;

    match result {
        Ok(word) => Redirect::to(&format!(
            "/languages/{}/words/{}/{}",
            language_code, word.slug, word.lemma
        ))
        .into_response(),
        Err(e) => {
            let template = NewWordTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word_classes: word_classes_list,
                previous_word: form.word.clone(),
                previous_word_class: form.word_class.clone(),
                previous_definition: definitions_text.first().map(|s| s.clone()).unwrap_or_default(),
                previous_definitions: definitions_text.iter().skip(1).map(|s| s.clone()).collect(),
                previous_context: form.contexts.first().map(|s| s.clone()).unwrap_or_default(),
                previous_contexts: form.contexts.iter().skip(1).map(|s| s.clone()).collect(),
                previous_ipa: form.ipa.clone().unwrap_or_default(),
                previous_notes: form.notes.clone().unwrap_or_default(),
                user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body).into_response()
        }
    }
}

async fn view_lemma(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let word = match words
        .find_by_slug_and_lemma(current_user.as_ref(), language.id, &slug, lemma)
        .await
    {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    // Fetch definitions for this word
    let (definitions, _has_more) = match definitions_repo
        .list_by_word(word.id, PaginatedRequest { limit: 100, offset: 0 })
        .await
    {
        Ok(res) => {
            (res.items, res.has_more)
        },
        Err(_) => (vec![], false),
    };

    let other_lemmata = definitions.len() > 1;

    // Construct the previous search URL (default to search page)
    let previous_search = format!("/languages/{}/words", language_code);

    let template = LemmaTemplate {
        current_user,
        language,
        word,
        definitions,
        other_lemmata,
        previous_search,
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn new_definition(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, word_id)): Path<(String, Uuid)>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let word = match words.find_by_id(word_id).await {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let template = NewDefinitionTemplate {
        current_user,
        error: None,
        language,
        word,
        previous_definition: String::new(),
        previous_context: String::new(),
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn new_definition_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, word_id)): Path<(String, Uuid)>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<DefinitionFormData>,
) -> impl IntoResponse {
    let Some(user) = s.user().cloned() else {
        return Redirect::to("/login").into_response();
    };

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let word = match words.find_by_id(word_id).await {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let create_def = CreateDefinition {
        definition: form.definition.clone(),
        context: form.context.clone(),
    };

    match definitions_repo.create(&user, word_id, create_def).await {
        Ok(_) => Redirect::to(&format!(
            "/languages/{}/words/{}/{}",
            language_code, word.slug, word.lemma
        ))
        .into_response(),
        Err(e) => {
            let template = NewDefinitionTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word,
                previous_definition: form.definition,
                previous_context: form.context.unwrap_or_default(),
                user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body).into_response()
        }
    }
}

async fn edit_definition(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, definition_id)): Path<(String, Uuid)>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let definition = match definitions_repo.find_by_id(definition_id).await {
        Ok(def) => def,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let word = match words.find_by_id(definition.word).await {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let template = EditDefinitionTemplate {
        current_user,
        error: None,
        language,
        word,
        definition: definition.clone(),
        previous_definition: definition.definition,
        previous_context: definition.context.unwrap_or_default(),
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn edit_definition_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, definition_id)): Path<(String, Uuid)>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<DefinitionFormData>,
) -> impl IntoResponse {
    let Some(user) = s.user().cloned() else {
        return Redirect::to("/login").into_response();
    };

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let definition = match definitions_repo.find_by_id(definition_id).await {
        Ok(def) => def,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let word = match words.find_by_id(definition.word).await {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let update = UpdateDefinition {
        definition: Some(form.definition.clone()),
        context: form.context.clone(),
    };

    match definitions_repo.update(&user, definition_id, update).await {
        Ok(_) => Redirect::to(&format!(
            "/languages/{}/words/{}/{}",
            language_code, word.slug, word.lemma
        ))
        .into_response(),
        Err(e) => {
            let template = EditDefinitionTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word,
                definition,
                previous_definition: form.definition,
                previous_context: form.context.unwrap_or_default(),
                user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body).into_response()
        }
    }
}

async fn delete_definition(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, definition_id)): Path<(String, Uuid)>,
) -> impl IntoResponse {
    let current_user = s.user().cloned();

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let definition = match definitions_repo.find_by_id(definition_id).await {
        Ok(def) => def,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let word = match words.find_by_id(definition.word).await {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(current_user.as_ref())(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let template = DeleteDefinitionTemplate {
        current_user,
        error: None,
        language,
        word,
        definition,
        user_has_permission,
    };

    let body = render_template(template);
    (StatusCode::OK, body).into_response()
}

async fn delete_definition_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, definition_id)): Path<(String, Uuid)>,
) -> impl IntoResponse {
    let Some(user) = s.user().cloned() else {
        return Redirect::to("/login").into_response();
    };

    let language = match languages.find_by_code(&language_code).await {
        Ok(lang) => lang,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let definition = match definitions_repo.find_by_id(definition_id).await {
        Ok(def) => def,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let word = match words.find_by_id(definition.word).await {
        Ok(w) => w,
        Err(e) => {
            let body = error_template(Some(&user))(e);
            return (StatusCode::NOT_FOUND, body).into_response();
        }
    };

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    match definitions_repo.delete(&user, definition_id).await {
        Ok(_) => Redirect::to(&format!(
            "/languages/{}/words/{}/{}",
            language_code, word.slug, word.lemma
        ))
        .into_response(),
        Err(e) => {
            let template = DeleteDefinitionTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word,
                definition,
                user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body).into_response()
        }
    }
}

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/languages/{language}/new-word", axum::routing::get(new_word))
        .route("/languages/{language}/new-word", axum::routing::post(new_word_submit))
        .route("/languages/{language}/words/{word_id}/new-definition", axum::routing::get(new_definition))
        .route("/languages/{language}/words/{word_id}/new-definition", axum::routing::post(new_definition_submit))
        .route("/languages/{language}/definitions/{definition_id}/edit", axum::routing::get(edit_definition))
        .route("/languages/{language}/definitions/{definition_id}/edit", axum::routing::post(edit_definition_submit))
        .route("/languages/{language}/definitions/{definition_id}/delete", axum::routing::get(delete_definition))
        .route("/languages/{language}/definitions/{definition_id}/delete", axum::routing::post(delete_definition_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{language}/words", axum::routing::get(word_search))
        .route("/languages/{language}/words/{slug}", axum::routing::get(view_lemmata))
        .route("/languages/{language}/words/{slug}/{lemma}", axum::routing::get(view_lemma));

    (secure_routes, normal_routes)
}
