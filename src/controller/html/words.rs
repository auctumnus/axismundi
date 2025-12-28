use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    err::{AppError, bad_request, not_found},
    get_user,
    model::{
        bookmarks::BookmarkRepository,
        definitions::{CreateDefinition, Definition, DefinitionRepository, UpdateDefinition},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_classes::{WordClass, WordClassRepository},
        word_relations::{
            CreateWordRelation, LeveledCognacy, RelationDirection, SearchWordRelations,
            WordRelationRepository, WordRelationSearchResult, WordRelationType,
        },
        words::{CreateWord, Word, WordRepository, WordSearch},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "words/search.html")]
#[allow(dead_code)]
struct WordSearchTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    previous_query: WordSearch,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<WordWithMeta>>,
    word_classes: Vec<WordClass>,
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
    rendered_notes: Vec<String>,
    creators: Vec<User>,
    contributor_counts: Vec<i64>,
    is_liked_list: Vec<bool>,
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
    rendered_notes: String,
    creator: User,
    contributor_count: i64,
    is_liked: bool,
    recent_relations: Vec<WordRelationSearchResult>,
    total_relations: i64,
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
    will_create_audit_log: bool,
}

#[derive(Template)]
#[template(path = "words/edit.html")]
#[allow(dead_code)]
struct EditWordTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: Language,
    word: Word,
    word_classes: Vec<WordClass>,
    previous_word: String,
    previous_word_class: String,
    previous_definitions: Vec<String>,
    previous_contexts: Vec<String>,
    previous_definition_ids: Vec<String>,
    previous_ipa: String,
    previous_notes: String,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

#[derive(Deserialize)]
struct PreviousSearchQuery {
    previous_search: Option<String>,
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
struct EditWordFormData {
    word: String,
    word_class: String,
    #[serde(default, rename = "definitions[]")]
    definitions: Vec<String>,
    #[serde(default, rename = "contexts[]")]
    contexts: Vec<String>,
    #[serde(default, rename = "definition_ids[]")]
    definition_ids: Vec<String>,
    ipa: Option<String>,
    notes: Option<String>,
}

struct WordWithMeta {
    word: Word,
    first_definition: Option<Definition>,
    creator: User,
}

async fn word_search(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    definitions: DefinitionRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    Query(query): Query<WordSearch>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let query = WordSearch {
        q: query.q.and_then(|q| {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        word_class: query.word_class.and_then(|wc| {
            let trimmed = wc.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        ..query
    };

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

    let results = match words
        .search(&language.id, pagination.clone(), query.clone())
        .await
    {
        Ok(res) => res,
        Err(e) => {
            let template = WordSearchTemplate {
                current_user,
                error: Some(e),
                language,
                previous_query: query,
                previous_pagination: pagination,
                results: None,
                word_classes: word_classes_list,
                user_has_permission,
            };
            let body = render_template(template);
            return (StatusCode::BAD_REQUEST, body);
        }
    };

    let mut results_with_meta = vec![];
    for word in results.items {
        let creator = attempt!(s, words.find_creator(&word.id).await);
        let first_definition = attempt!(s, definitions.get_first_by_word(&word.id).await);
        results_with_meta.push(WordWithMeta {
            word,
            first_definition,
            creator,
        });
    }

    let results_with_meta = Some(PaginatedResponse {
        items: results_with_meta,
        total: results.total,
        limit: results.limit,
        offset: results.offset,
        has_more: results.has_more,
    });

    let template = WordSearchTemplate {
        current_user,
        error: None,
        language,
        previous_query: query,
        previous_pagination: pagination,
        results: results_with_meta,
        word_classes: word_classes_list,
        user_has_permission,
    };

    let body = render_template(template);
    okay(body)
}

async fn view_lemmata(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod || permissions
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

    let lemmata = attempt!(
        s,
        words
            .search(&language.id, PaginatedRequest::default(), search)
            .await
    )
    .items;

    if lemmata.is_empty() {
        return render_generic_error(s, not_found(format!("word with slug '{slug}'"))).await;
    }

    // If there's only one lemma, redirect to it directly
    if lemmata.len() == 1 {
        let lemma = &lemmata[0];
        return (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/words/{}/{}",
                language_code, slug, lemma.lemma
            ))
            .into_response(),
        );
    }

    let word = lemmata[0].word.clone();

    // Fetch the word class names and definitions for each lemma
    let mut parts_of_speech = Vec::new();
    let mut words_definitions = Vec::new();
    let mut rendered_notes = Vec::new();
    let mut creators = Vec::new();
    let mut contributor_counts = Vec::new();
    let mut is_liked_list = Vec::new();

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
            .list_by_word(
                lemma.id,
                PaginatedRequest {
                    limit: 10,
                    offset: 0,
                },
            )
            .await
        {
            Ok(res) => res.items,
            Err(_) => vec![],
        };
        words_definitions.push(definitions);

        // Render notes for this lemma
        let notes = attempt!(s, words.render_notes(lemma).await);
        rendered_notes.push(notes);

        // Fetch creator
        let creator = attempt!(s, words.find_creator(&lemma.id).await);
        creators.push(creator);

        // Fetch contributor count
        let contributor_count = attempt!(s, words.count_contributors(lemma.id).await);
        contributor_counts.push(contributor_count);

        // Check if liked by current user
        let is_liked = if let Some(user) = &current_user {
            words.is_liked(&lemma.id, &user.id).await.unwrap_or(false)
        } else {
            false
        };
        is_liked_list.push(is_liked);

        println!(
            "Lemma ID: {}, Definitions: {:?}",
            lemma.id,
            words_definitions.last()
        );
    }

    let template = LemmataTemplate {
        current_user,
        language,
        word,
        lemmata,
        parts_of_speech,
        words_definitions,
        user_has_permission,
        rendered_notes,
        creators,
        contributor_counts,
        is_liked_list,
    };

    let body = render_template(template);
    okay(body)
}

async fn new_word(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let will_create_audit_log = if let Some(user) = &current_user {
        crate::util::will_create_audit_log_for_language(&state, user, language.id).await
    } else {
        false
    };

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

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
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

async fn new_word_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

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
            error: Some(crate::err::bad_request(
                "At least one definition is required",
            )),
            language,
            word_classes: word_classes_list,
            previous_word: form.word.clone(),
            previous_word_class: form.word_class.clone(),
            previous_definition: form
                .definitions
                .first()
                .map(|s| s.clone())
                .unwrap_or_default(),
            previous_definitions: form.definitions.iter().skip(1).map(|s| s.clone()).collect(),
            previous_context: form.contexts.first().map(|s| s.clone()).unwrap_or_default(),
            previous_contexts: form.contexts.iter().skip(1).map(|s| s.clone()).collect(),
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            user_has_permission,
            will_create_audit_log,
        };

        let body = render_template(template);
        return (StatusCode::BAD_REQUEST, body);
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
        Ok(word) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/words/{}/{}",
                language_code, word.slug, word.lemma
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = NewWordTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word_classes: word_classes_list,
                previous_word: form.word.clone(),
                previous_word_class: form.word_class.clone(),
                previous_definition: definitions_text
                    .first()
                    .map(|s| s.clone())
                    .unwrap_or_default(),
                previous_definitions: definitions_text.iter().skip(1).map(|s| s.clone()).collect(),
                previous_context: form.contexts.first().map(|s| s.clone()).unwrap_or_default(),
                previous_contexts: form.contexts.iter().skip(1).map(|s| s.clone()).collect(),
                previous_ipa: form.ipa.clone().unwrap_or_default(),
                previous_notes: form.notes.clone().unwrap_or_default(),
                user_has_permission,
                will_create_audit_log,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

async fn view_lemma(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    word_relations: WordRelationRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(params): Query<PreviousSearchQuery>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(current_user.as_ref(), language.id, &slug, lemma)
            .await
    );

    // Fetch definitions for this word
    let (definitions, _has_more) = match definitions_repo
        .list_by_word(
            word.id,
            PaginatedRequest {
                limit: 100,
                offset: 0,
            },
        )
        .await
    {
        Ok(res) => (res.items, res.has_more),
        Err(_) => (vec![], false),
    };

    let other_lemmata = attempt!(s, words.count_by_slug(language.id, &slug).await) > 1;

    // Construct the previous search URL
    let previous_search = if let Some(search_params) = params.previous_search {
        format!("/languages/{}/words?{}", language_code, search_params)
    } else {
        format!("/languages/{}/words", language_code)
    };

    let rendered_notes = attempt!(s, words.render_notes(&word).await);

    let creator = attempt!(s, words.find_creator(&word.id).await);
    let contributor_count = attempt!(s, words.count_contributors(word.id).await);

    let is_liked = if let Some(user) = &current_user {
        words.is_liked(&word.id, &user.id).await.unwrap_or(false)
    } else {
        false
    };

    // Fetch recent word relations (3 most recent, with cognacy relations first)
    let relations_pagination = PaginatedRequest {
        limit: 3,
        offset: 0,
    };
    let relations_search = SearchWordRelations {
        q: None,
        kind: None,
        direction: None,
    };
    let relations_result = word_relations
        .search(relations_pagination, relations_search, &word)
        .await;
    let (recent_relations, total_relations) = match relations_result {
        Ok(res) => (res.items, res.total),
        Err(_) => (vec![], 0),
    };

    let template = LemmaTemplate {
        current_user,
        language,
        word,
        definitions,
        other_lemmata,
        previous_search,
        user_has_permission,
        rendered_notes,
        creator,
        contributor_count,
        is_liked,
        recent_relations,
        total_relations,
    };

    let body = render_template(template);
    okay(body)
}

async fn edit_word(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(current_user.as_ref(), language.id, &slug, lemma)
            .await
    );

    let user_has_permission = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let will_create_audit_log = if let Some(user) = &current_user {
        crate::util::will_create_audit_log_for_language(&state, user, language.id).await
    } else {
        false
    };

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

    // Get current word class abbreviation
    let word_class_abbr = if let Some(wc_id) = word.word_class {
        match word_classes.find_by_id(wc_id).await {
            Ok(wc) => wc.abbreviation,
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    // Fetch existing definitions
    let definitions_result = match definitions_repo
        .list_by_word(
            word.id,
            PaginatedRequest {
                limit: 100,
                offset: 0,
            },
        )
        .await
    {
        Ok(res) => res.items,
        Err(_) => vec![],
    };

    let previous_definitions: Vec<String> = definitions_result
        .iter()
        .map(|d| d.definition.clone())
        .collect();
    let previous_contexts: Vec<String> = definitions_result
        .iter()
        .map(|d| d.context.clone().unwrap_or_default())
        .collect();
    let previous_definition_ids: Vec<String> = definitions_result
        .iter()
        .map(|d| d.id.to_string())
        .collect();

    let template = EditWordTemplate {
        current_user,
        error: None,
        language,
        word: word.clone(),
        word_classes: word_classes_list,
        previous_word: word.word.clone(),
        previous_word_class: word_class_abbr,
        previous_definitions,
        previous_contexts,
        previous_definition_ids,
        previous_ipa: word.ipa.unwrap_or_default(),
        previous_notes: word.notes.unwrap_or_default(),
        user_has_permission,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

async fn edit_word_submit(
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
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&user), language.id, &slug, lemma)
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

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
        let template = EditWordTemplate {
            current_user: Some(user),
            error: Some(crate::err::bad_request(
                "At least one definition is required",
            )),
            language,
            word,
            word_classes: word_classes_list,
            previous_word: form.word.clone(),
            previous_word_class: form.word_class.clone(),
            previous_definitions: form.definitions.clone(),
            previous_contexts: form.contexts.clone(),
            previous_definition_ids: form.definition_ids.clone(),
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            user_has_permission,
            will_create_audit_log,
        };

        let body = render_template(template);
        return (StatusCode::BAD_REQUEST, body);
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
        let updated_word = words
            .update_by_lemma(&user, language.id, &slug, lemma, update_word)
            .await?;

        // Handle definitions: update existing, create new, delete removed
        let existing_defs = definitions_repo
            .list_by_word(
                updated_word.id,
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
                definitions_repo.update(&user, *def_id, update).await?;
            } else {
                // Create new definition
                let create_def = CreateDefinition {
                    definition: def_text.clone(),
                    context,
                };
                definitions_repo
                    .create(&user, updated_word.id, create_def)
                    .await?;
            }
        }

        // Delete definitions that were removed
        for existing_def in existing_defs {
            if !kept_ids.contains(&existing_def.id) {
                definitions_repo.delete(&user, existing_def.id).await?;
            }
        }

        Ok::<_, crate::err::AppError>(updated_word)
    }
    .await;

    match result {
        Ok(updated_word) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/words/{}/{}",
                language_code, updated_word.slug, updated_word.lemma
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = EditWordTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word,
                word_classes: word_classes_list,
                previous_word: form.word.clone(),
                previous_word_class: form.word_class.clone(),
                previous_definitions: definitions_text,
                previous_contexts: form.contexts.clone(),
                previous_definition_ids: form.definition_ids.clone(),
                previous_ipa: form.ipa.clone().unwrap_or_default(),
                previous_notes: form.notes.clone().unwrap_or_default(),
                user_has_permission,
                will_create_audit_log,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "words/add_relation.html")]
#[allow(dead_code)]
struct AddRelationTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    error: Option<AppError>,
    will_create_audit_log: bool,
}

async fn add_relation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    // Check permission
    let has_permission = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    if !has_permission {
        return render_generic_error(s, bad_request("You don't have permission to add relations"))
            .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    let template = AddRelationTemplate {
        current_user: Some(current_user.clone()),
        language,
        word,
        error: None,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

async fn add_relation_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    bookmarks: BookmarkRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Form(form): Form<AddRelationForm>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    // Check permission on source language
    let has_permission = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    if !has_permission {
        let template = AddRelationTemplate {
            current_user: Some(current_user.clone()),
            language,
            word,
            error: Some(bad_request("You don't have permission to add relations")),
            will_create_audit_log,
        };
        let body = render_template(template);
        return okay(body);
    }

    // Look up the target word by bookmark
    let bookmark_result = bookmarks.get_by_slug(&form.target_bookmark).await;

    let target_word = match bookmark_result {
        Ok(bookmark) => {
            // Get the target word using the bookmark's item UUID
            match words.find_by_id(bookmark.item).await {
                Ok(w) => w,
                Err(e) => {
                    let template = AddRelationTemplate {
                        current_user: Some(current_user.clone()),
                        language,
                        word,
                        error: Some(e),
                        will_create_audit_log,
                    };
                    let body = render_template(template);
                    return okay(body);
                }
            }
        }
        Err(e) => {
            let template = AddRelationTemplate {
                current_user: Some(current_user.clone()),
                language,
                word,
                error: Some(e),
                will_create_audit_log,
            };
            let body = render_template(template);
            return okay(body);
        }
    };

    // Check permission on target language
    let has_target_permission = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(
                current_user.id,
                target_word.language,
                PermissionLevel::Editor
            )
            .await
    );
    if !has_target_permission {
        let template = AddRelationTemplate {
            current_user: Some(current_user.clone()),
            language,
            word,
            error: Some(bad_request(
                "You don't have permission to edit the target word's language",
            )),
            will_create_audit_log,
        };
        let body = render_template(template);
        return okay(body);
    }

    // Create the relation
    let relation = CreateWordRelation {
        antecedent: word.clone(),
        consequent: target_word.clone(),
        kind: form.kind,
    };

    let relation_result = word_relations.create(&current_user, relation).await;

    match relation_result {
        Ok(_) => {
            // Redirect to etymology page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language.code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => {
            let template = AddRelationTemplate {
                current_user: Some(current_user.clone()),
                language,
                word,
                error: Some(e),
                will_create_audit_log,
            };
            let body = render_template(template);
            okay(body)
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AddRelationForm {
    kind: WordRelationType,
    target_bookmark: String,
}

#[derive(Template)]
#[template(path = "words/relations.html")]
#[allow(dead_code)]
struct WordRelationsTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    results: Option<PaginatedResponse<WordRelationSearchResult>>,
    previous_query: SearchWordRelations,
    previous_pagination: PaginatedRequest,
    error: Option<AppError>,
    user_has_permission: bool,
}

#[derive(Debug, Deserialize)]
struct RelationsFilterQuery {
    kind: Option<String>,
    direction: Option<String>,
}

async fn view_word_relations(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(filter): Query<RelationsFilterQuery>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(current_user.as_ref(), language.id, &slug, lemma)
            .await
    );

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod || language_permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    // Parse filter parameters
    let kind: Option<WordRelationType> = filter.kind.as_ref().and_then(|k| {
        if k.is_empty() {
            None
        } else {
            serde_json::from_value(serde_json::Value::String(k.clone())).ok()
        }
    });

    let direction: Option<RelationDirection> = filter.direction.as_ref().and_then(|d| {
        if d.is_empty() {
            None
        } else {
            use std::str::FromStr;
            RelationDirection::from_str(d).ok()
        }
    });

    let search = SearchWordRelations {
        q: None,
        kind,
        direction,
    };

    let results = match word_relations
        .search(pagination.clone(), search.clone(), &word)
        .await
    {
        Ok(res) => Some(res),
        Err(e) => {
            let template = WordRelationsTemplate {
                current_user,
                language,
                word,
                results: None,
                previous_query: search,
                previous_pagination: pagination,
                error: Some(e),
                user_has_permission,
            };
            let body = render_template(template);
            return (StatusCode::BAD_REQUEST, body);
        }
    };

    let template = WordRelationsTemplate {
        current_user,
        language,
        word,
        results,
        previous_query: search,
        previous_pagination: pagination,
        error: None,
        user_has_permission,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "words/delete_relation.html")]
#[allow(dead_code)]
struct DeleteRelationTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    related_word: Word,
    related_word_language_code: String,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn delete_relation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    // Check permission
    let has_permission = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    if !has_permission {
        return render_generic_error(
            s,
            bad_request("You don't have permission to delete relations"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    let template = DeleteRelationTemplate {
        current_user: Some(current_user.clone()),
        language,
        word,
        related_word,
        related_word_language_code: related_language_code.clone(),
        user_has_permission: has_permission,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

async fn delete_relation_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    // Check permission
    let has_permission = attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    if !has_permission {
        return render_generic_error(
            s,
            bad_request("You don't have permission to delete relations"),
        )
        .await;
    }

    // Delete the relation
    match word_relations
        .delete(&current_user, &word, &related_word)
        .await
    {
        Ok(_) => {
            // Redirect back to the word page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language.code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => render_generic_error(s, e).await,
    }
}

#[derive(Template)]
#[template(path = "words/edit_relation.html")]
#[allow(dead_code)]
struct EditRelationTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    related_word: Word,
    related_language: Language,
    related_word_language_code: String,
    relation_kind: String,
    error: Option<AppError>,
    user_has_permission: bool,
    user_has_permission_on_related: bool,
    will_create_audit_log: bool,
}

async fn edit_relation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    // Check permission
    let has_permission = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    if !has_permission {
        return render_generic_error(
            s,
            bad_request("You don't have permission to edit relations"),
        )
        .await;
    }

    let has_permission_on_related = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(
                current_user.id,
                related_language.id,
                PermissionLevel::Editor
            )
            .await
    );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    // Find the existing relation to get its current kind
    let relation_kind = match word_relations.find_relation(&word, &related_word).await {
        Ok(relation) => relation.kind.to_string(),
        Err(e) => return render_generic_error(s, e).await,
    };

    let template = EditRelationTemplate {
        current_user: Some(current_user.clone()),
        language,
        word,
        related_word,
        related_language,
        related_word_language_code: related_language_code.clone(),
        relation_kind,
        error: None,
        user_has_permission: has_permission,
        user_has_permission_on_related: has_permission_on_related,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Debug, serde::Deserialize)]
struct EditRelationForm {
    kind: WordRelationType,
}

async fn edit_relation_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
    Form(form): Form<EditRelationForm>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    // Check permission
    let has_permission = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    let has_permission_on_related = is_admin_or_mod || attempt!(
        s,
        language_permissions
            .has_permission(
                current_user.id,
                related_language.id,
                PermissionLevel::Editor
            )
            .await
    );
    if !has_permission {
        let template = EditRelationTemplate {
            current_user: Some(current_user.clone()),
            language,
            word,
            related_word,
            related_language,
            related_word_language_code: related_language_code.clone(),
            relation_kind: form.kind.to_string(),
            error: Some(bad_request("You don't have permission to edit relations")),
            user_has_permission: has_permission,
            user_has_permission_on_related: has_permission_on_related,
            will_create_audit_log,
        };
        let body = render_template(template);
        return okay(body);
    }

    // Update the relation
    match word_relations
        .update(&current_user, &word, &related_word, form.kind)
        .await
    {
        Ok(_) => {
            // Redirect back to the word page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language.code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => {
            let template = EditRelationTemplate {
                current_user: Some(current_user.clone()),
                language,
                word,
                related_word,
                related_language,
                related_word_language_code: related_language_code.clone(),
                relation_kind: form.kind.to_string(),
                error: Some(e),
                user_has_permission: has_permission,
                user_has_permission_on_related: has_permission_on_related,
                will_create_audit_log,
            };
            let body = render_template(template);
            okay(body)
        }
    }
}

#[derive(Template)]
#[template(path = "words/delete.html")]
#[allow(dead_code)]
struct DeleteWordTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn delete_word_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&user), language.id, &slug, lemma)
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod || permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeleteWordTemplate {
        current_user: Some(user),
        language,
        word,
        user_has_permission,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_word_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    match words
        .delete_by_lemma(&user, language.id, &slug, lemma)
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/words", language_code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/languages/{language}/new-word", axum::routing::get(new_word))
        .route("/languages/{language}/words/{slug}/{lemma}/add-relation", axum::routing::post(add_relation_submit))
        .route("/languages/{language}/new-word", axum::routing::post(new_word_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/edit", axum::routing::get(edit_word))
        .route("/languages/{language}/words/{slug}/{lemma}/edit", axum::routing::post(edit_word_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/delete", axum::routing::post(delete_word_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/delete", axum::routing::post(delete_relation_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/edit", axum::routing::post(edit_relation_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{language}/words", axum::routing::get(word_search))
        .route("/languages/{language}/words/{slug}", axum::routing::get(view_lemmata))
        .route("/languages/{language}/words/{slug}/{lemma}", axum::routing::get(view_lemma))
        .route("/languages/{language}/words/{slug}/{lemma}/relations", axum::routing::get(view_word_relations))
        .route("/languages/{language}/words/{slug}/{lemma}/add-relation", axum::routing::get(add_relation_form))
        .route("/languages/{language}/words/{slug}/{lemma}/delete", axum::routing::get(delete_word_form))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/delete", axum::routing::get(delete_relation_form))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/edit", axum::routing::get(edit_relation_form));

    (secure_routes, normal_routes)
}
