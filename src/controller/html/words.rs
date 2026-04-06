use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use futures::TryFutureExt;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{self, okay, render_generic_error, render_template},
    embed::{EmbedTarget, GenericEmbed, render_embed, truncate_description},
    err::{AppError, bad_request, forbidden, not_found},
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
            CreateWordRelation, SearchWordRelations, WordRelationRepository,
            WordRelationSearchResult, WordRelationType,
        },
        sound_change_sets::{SoundChangeSet, SoundChangeSetRepository},
        words::{CreateWord, Word, WordRepository, WordSearch, WordWithMeta},
    },
    pagination::PaginatedRequest,
    util::{AppState, BackQuery, ListHeaderKind, extract_session::Session, search_template::{SearchTemplateArgs, make_search_layout}},
};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "words/fragments/list_header.html")]
struct Header<'a> {
    can_edit_language: bool,
    language: &'a Language,
    kind: ListHeaderKind,
}

impl Header<'_> {
    fn title(&self) -> &'static str {
        match self.kind {
            ListHeaderKind::Preview => "words",
            ListHeaderKind::Search => "search words",
        }
    }
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
    back: String,
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
    ipa_estimator: Option<SoundChangeSet>,
    antecedent_bookmark: String,
    relation_kind: String,
    antecedent: Option<AntecedentContext>,
}

#[derive(Deserialize, Default)]
struct NewWordPrefill {
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

#[derive(Template)]
#[template(path = "words/derive_into_family.html")]
#[allow(dead_code)]
struct DeriveIntoFamilyTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    descendants: Vec<DescendantOption>,
    loan_options: Vec<Language>,
    single_family: bool,
    error: Option<AppError>,
}

struct AntecedentContext {
    word: Word,
    language: Language,
}

async fn lookup_antecedent(
    bookmarks: &BookmarkRepository,
    words: &WordRepository,
    languages: &LanguageRepository,
    antecedent_bookmark: &str,
) -> Option<AntecedentContext> {
    if antecedent_bookmark.is_empty() {
        return None;
    }
    let bookmark = bookmarks.get_by_slug(antecedent_bookmark).await.ok()?;
    let word = words.find_by_id(bookmark.item).await.ok()?;
    let language_code = word.language_code.clone()?;
    let language = languages.find_by_code(&language_code).await.ok()?;
    Some(AntecedentContext { word, language })
}

struct DescendantOption {
    language_code: String,
    language_name: String,
    family_name: String,
}

#[derive(Deserialize, Default)]
struct DeriveQuery {
    descendant: Option<String>,
}

#[derive(Deserialize)]
struct DeriveIntoFamilyLoanForm {
    target_language_code: String,
    relation_kind: WordRelationType,
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
    ipa_estimator: Option<SoundChangeSet>,
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
    antecedent_bookmark: Option<String>,
    relation_kind: Option<String>,
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

#[derive(Template)]
#[template(path = "words/fragments/query.html")]
struct WordSearchOptions {
    query: WordSearch,
    word_classes: Vec<WordClass>,
}

#[derive(Template)]
#[template(path = "words/fragments/card.html")]
struct PreviewCard<'a> {
    word_with_meta: WordWithMeta,
    back_url: &'a str,
}

#[allow(clippy::too_many_arguments)]
async fn word_search(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    Query(query): Query<WordSearch>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let search_action = format!("/languages/{}/words", language.code);

    let back_url = crate::util::back_url(
        &search_action,
        &pagination,
        &query,
    );

    let can_edit_language = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod
            || permissions
                .has_permission(user.id, language.id, PermissionLevel::Editor)
                .await
                .unwrap_or(false)
    } else {
        false
    };

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);
    let query_template = WordSearchOptions {
        query: query.clone(),
        word_classes: word_classes_list,
    };

    let breadcrumbs = html::languages::Breadcrumb { language: &language };

    let results = words
        .search(&language.id, pagination.clone(), query.clone())
        .and_then(|results| results.try_map_async(|word| words.materialize(word, s.user())))
        .await;

    let render_item = |word_with_meta: &WordWithMeta|
        PreviewCard {
            word_with_meta: word_with_meta.clone(),
            back_url: &back_url,
        };
    
    let header = Header {
        can_edit_language,
        language: &language,
        kind: ListHeaderKind::Search,
    };

    let footer = html::languages::Footer {
        can_edit_language,
        language: &language,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template,
        query,
        results,
        pagination,
        search_name: "words",
        search_action,
        render_item,
    }).with_breadcrumbs(breadcrumbs).with_footer(footer);

    let status = template.status();

    (status, render_template(template))
}

#[allow(clippy::too_many_arguments)]
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

        is_admin_or_mod
            || permissions
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
        let notes = attempt!(s, WordRepository::render_notes(lemma));
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

#[allow(clippy::too_many_arguments)]
async fn new_word(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    bookmarks: BookmarkRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
    axum_extra::extract::Query(prefill): axum_extra::extract::Query<NewWordPrefill>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod
            || permissions
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
    let ipa_estimator = languages.get_ipa_estimator(language.id).await.ok().flatten();

    let previous_definition = prefill.definitions.first().cloned().unwrap_or_default();
    let previous_definitions = prefill.definitions.iter().skip(1).cloned().collect();
    let previous_context = prefill.contexts.first().cloned().unwrap_or_default();
    let previous_contexts = prefill.contexts.iter().skip(1).cloned().collect();

    let antecedent_bookmark_str = prefill.antecedent_bookmark.unwrap_or_default();
    let antecedent = lookup_antecedent(&bookmarks, &words, &languages, &antecedent_bookmark_str).await;

    let template = NewWordTemplate {
        current_user,
        error: None,
        language,
        word_classes: word_classes_list,
        previous_word: prefill.word.unwrap_or_default(),
        previous_word_class: prefill.word_class.unwrap_or_default(),
        previous_definition,
        previous_definitions,
        previous_context,
        previous_contexts,
        previous_ipa: prefill.ipa.unwrap_or_default(),
        previous_notes: String::new(),
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
        antecedent_bookmark: antecedent_bookmark_str,
        relation_kind: prefill.relation_kind.unwrap_or_default(),
        antecedent,
    };

    let body = render_template(template);
    okay(body)
}

#[allow(clippy::too_many_arguments)]
async fn new_word_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    permissions: LanguagePermissionRepository,
    word_relations: WordRelationRepository,
    bookmarks: BookmarkRepository,
    Path(language_code): Path<String>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> (StatusCode, Response) {
    const MAX_DEFINITIONS: usize = 10;
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);
    let ipa_estimator = languages.get_ipa_estimator(language.id).await.ok().flatten();

    let antecedent_bookmark = form.antecedent_bookmark.clone().unwrap_or_default();
    let relation_kind_str = form.relation_kind.clone().unwrap_or_default();

    // Filter out empty definitions and limit to 10
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
        let antecedent = lookup_antecedent(&bookmarks, &words, &languages, &antecedent_bookmark).await;
        let template = NewWordTemplate {
            current_user: Some(user),
            error: Some(crate::err::bad_request(
                "At least one definition is required",
            )),
            language,
            word_classes: word_classes_list,
            previous_word: form.word.clone(),
            previous_word_class: form.word_class.clone(),
            previous_definition: form.definitions.first().cloned().unwrap_or_default(),
            previous_definitions: form.definitions.iter().skip(1).cloned().collect(),
            previous_context: form.contexts.first().cloned().unwrap_or_default(),
            previous_contexts: form.contexts.iter().skip(1).cloned().collect(),
            previous_ipa: form.ipa.clone().unwrap_or_default(),
            previous_notes: form.notes.clone().unwrap_or_default(),
            user_has_permission,
            will_create_audit_log,
            ipa_estimator,
            antecedent_bookmark,
            relation_kind: relation_kind_str,
            antecedent,
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
        Ok(word) => {
            // Optionally create word relation if antecedent was provided
            let bm = form.antecedent_bookmark.as_deref().unwrap_or("").trim();
            let kind_str = form.relation_kind.as_deref().unwrap_or("").trim();
            if !bm.is_empty() && !kind_str.is_empty() {
                if let Some(kind) = parse_word_relation_type(kind_str) {
                    if let Ok(bookmark) = bookmarks.get_by_slug(bm).await {
                        if let Ok(source_word) = words.find_by_id(bookmark.item).await {
                            let relation = CreateWordRelation {
                                antecedent: source_word,
                                consequent: word.clone(),
                                kind,
                            };
                            let _ = word_relations.create(&user, relation).await;
                        }
                    }
                }
            }

            (
                StatusCode::SEE_OTHER,
                Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language_code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => {
            let antecedent = lookup_antecedent(&bookmarks, &words, &languages, &antecedent_bookmark).await;
            let template = NewWordTemplate {
                current_user: Some(user),
                error: Some(e),
                language,
                word_classes: word_classes_list,
                previous_word: form.word.clone(),
                previous_word_class: form.word_class.clone(),
                previous_definition: definitions_text.first().cloned().unwrap_or_default(),
                previous_definitions: definitions_text.iter().skip(1).cloned().collect(),
                previous_context: form.contexts.first().cloned().unwrap_or_default(),
                previous_contexts: form.contexts.iter().skip(1).cloned().collect(),
                previous_ipa: form.ipa.clone().unwrap_or_default(),
                previous_notes: form.notes.clone().unwrap_or_default(),
                user_has_permission,
                will_create_audit_log,
                ipa_estimator,
                antecedent_bookmark,
                relation_kind: relation_kind_str,
                antecedent,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn view_lemma(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    word_relations: WordRelationRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(params): Query<BackQuery>,
    user_agent: Option<TypedHeader<UserAgent>>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod
            || permissions
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

    let back = params.back.unwrap_or_default();

    let rendered_notes = attempt!(s, WordRepository::render_notes(&word));

    let creator = attempt!(s, words.find_creator(&word.id).await);

    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        println!("hi discord!");

        let title = if let Some(word_class) = word.word_class_abbreviation {
            format!("{} ({}.)", word.word, word_class)
        } else {
            word.word.clone()
        };

        let url = format!(
            "{}/languages/{}/words/{}/{}",
            crate::CONFIG.public_url_base,
            language.code,
            word.slug,
            word.lemma
        );

        let rendered_definitions = definitions
            .iter()
            .enumerate()
            .take(3)
            .map(|(i, d)| {
                let i = i + 1;
                format!(
                    "{i}. {}{}",
                    if d.context.is_empty() {
                        String::new()
                    } else {
                        format!("({}) ", d.context)
                    },
                    d.definition
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        let notes = if word.notes.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", word.notes)
        };

        let combined = format!("{rendered_definitions}{notes}");
        let description = format!(
            "{}\n\n⭐️ {}",
            truncate_description(&combined),
            word.like_count
        );

        return okay(
            render_embed(
                EmbedTarget::Discord,
                GenericEmbed {
                    url,
                    title,
                    description,
                    author: Some(creator),
                    image: None,
                    color: None,
                },
            )
            .await
            .into_response(),
        );
    }

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
        back,
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

#[allow(clippy::too_many_arguments)]
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
    let ipa_estimator = languages.get_ipa_estimator(language.id).await.ok().flatten();

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
        .map(|d| d.context.clone())
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
        previous_ipa: word.ipa.clone(),
        previous_notes: word.notes.clone(),
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
    };

    let body = render_template(template);
    okay(body)
}

#[allow(clippy::too_many_arguments)]
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
    const MAX_DEFINITIONS: usize = 10;
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

    let user_has_permission = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);
    let ipa_estimator = languages.get_ipa_estimator(language.id).await.ok().flatten();

    // Filter out empty definitions and limit to 10
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
            ipa_estimator,
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
                definitions_repo.update(&user, *def_id, update).await?;
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
                ipa_estimator,
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
    let has_permission = is_admin_or_mod
        || attempt!(
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

#[allow(clippy::too_many_arguments)]
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
    let has_permission = is_admin_or_mod
        || attempt!(
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
    let has_target_permission = is_admin_or_mod
        || attempt!(
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
#[template(path = "words/relations/fragments/list_header.html")]
struct RelationsHeader<'a> {
    user_has_permission: bool,
    language: &'a Language,
    word: &'a Word,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/query.html")]
struct RelationsQueryTemplate {
    query: SearchWordRelations,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/card.html")]
struct RelationCard<'a> {
    relation: WordRelationSearchResult,
    back_url: &'a str,
    current_word_language_code: &'a str,
    current_word_slug: &'a str,
    current_word_lemma: i32,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/breadcrumb.html")]
struct RelationsBreadcrumb<'a> {
    language: &'a Language,
    word: &'a Word,
}


#[allow(clippy::too_many_arguments)]
async fn view_word_relations(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(query): Query<SearchWordRelations>,
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

        is_admin_or_mod
            || language_permissions
                .has_permission(user.id, language.id, PermissionLevel::Editor)
                .await
                .unwrap_or(false)
    } else {
        false
    };

    let search_action = format!(
        "/languages/{}/words/{}/{}/relations",
        language.code, word.slug, word.lemma
    );

    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let header = RelationsHeader {
        user_has_permission,
        language: &language,
        word: &word,
    };

    let query_template = RelationsQueryTemplate {
        query: query.clone(),
    };

    let breadcrumbs = RelationsBreadcrumb {
        language: &language,
        word: &word,
    };

    let footer = html::languages::Footer {
        language: &language,
        can_edit_language: user_has_permission,
    };

    let results = word_relations
        .search(pagination.clone(), query.clone(), &word)
        .await;

    let render_item = |relation: &WordRelationSearchResult| RelationCard {
        relation: relation.clone(),
        back_url: &back_url,
        current_word_language_code: &language.code,
        current_word_slug: &word.slug,
        current_word_lemma: word.lemma,
        user_has_permission,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template,
        query,
        results,
        pagination,
        search_name: "relations",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();
    (status, render_template(template))
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
    let has_permission = is_admin_or_mod
        || attempt!(
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
        Ok(()) => {
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
    let has_permission = is_admin_or_mod
        || attempt!(
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

    let has_permission_on_related = is_admin_or_mod
        || attempt!(
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

#[allow(clippy::too_many_arguments)]
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
    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    let has_permission_on_related = is_admin_or_mod
        || attempt!(
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

    let user_has_permission = is_admin_or_mod
        || permissions
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

#[allow(clippy::too_many_arguments)]
async fn estimate_ipa_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    sets: crate::model::sound_change_sets::SoundChangeSetRepository,
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
    let user_has_permission = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);
    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;
    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

    let ipa_estimator = languages.get_ipa_estimator(language.id).await.ok().flatten();
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

#[allow(clippy::too_many_arguments)]
async fn estimate_ipa_new_word(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    bookmarks: BookmarkRepository,
    permissions: LanguagePermissionRepository,
    sets: crate::model::sound_change_sets::SoundChangeSetRepository,
    Path(language_code): Path<String>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<NewWordFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);
    let user_has_permission = is_admin_or_mod
        || permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);
    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;
    let word_classes_list = attempt!(s, word_classes.list_all(language.id).await);

    let ipa_estimator = languages.get_ipa_estimator(language.id).await.ok().flatten();
    let estimated_ipa = match &ipa_estimator {
        Some(scs) => match sets.run_from_db(&scs.id, vec![form.word.clone()]).await {
            Ok(response) => response.output_words.into_iter().next().unwrap_or_default(),
            Err(_) => form.ipa.clone().unwrap_or_default(),
        },
        None => form.ipa.clone().unwrap_or_default(),
    };

    let previous_definition = form.definitions.first().cloned().unwrap_or_default();
    let previous_definitions = form.definitions.iter().skip(1).cloned().collect();
    let previous_context = form.contexts.first().cloned().unwrap_or_default();
    let previous_contexts = form.contexts.iter().skip(1).cloned().collect();

    let antecedent_bookmark_str = form.antecedent_bookmark.clone().unwrap_or_default();
    let antecedent = lookup_antecedent(&bookmarks, &words, &languages, &antecedent_bookmark_str).await;

    let template = NewWordTemplate {
        current_user: Some(user),
        error: None,
        language,
        word_classes: word_classes_list,
        previous_word: form.word.clone(),
        previous_word_class: form.word_class.clone(),
        previous_definition,
        previous_definitions,
        previous_context,
        previous_contexts,
        previous_ipa: estimated_ipa,
        previous_notes: form.notes.clone().unwrap_or_default(),
        user_has_permission,
        will_create_audit_log,
        ipa_estimator,
        antecedent_bookmark: antecedent_bookmark_str,
        relation_kind: form.relation_kind.clone().unwrap_or_default(),
        antecedent,
    };

    let body = render_template(template);
    okay(body)
}

fn encode_query_value(s: &str) -> String {
    percent_encoding::percent_encode(s.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn parse_word_relation_type(s: &str) -> Option<WordRelationType> {
    match s {
        "derived" => Some(WordRelationType::Derived),
        "descendant" => Some(WordRelationType::Descendant),
        "compound" => Some(WordRelationType::Compound),
        "calque" => Some(WordRelationType::Calque),
        "borrowed" => Some(WordRelationType::Borrowed),
        "related" => Some(WordRelationType::Related),
        "see_also" => Some(WordRelationType::SeeAlso),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn derive_into_family_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    language_permissions: LanguagePermissionRepository,
    scs: SoundChangeSetRepository,
    definitions_repo: DefinitionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(params): Query<DeriveQuery>,
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

    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    if !has_permission {
        return render_generic_error(
            s,
            forbidden("You don't have permission to derive words from this language"),
        )
        .await;
    }

    if let Some(descendant_code) = params.descendant {
        // Derive descendant flow: compute derived form and redirect to new-word
        let target_language = attempt!(s, languages.find_by_code(&descendant_code).await);

        let has_target_permission = is_admin_or_mod
            || attempt!(
                s,
                language_permissions
                    .has_permission(current_user.id, target_language.id, PermissionLevel::Editor)
                    .await
            );
        if !has_target_permission {
            return render_generic_error(
                s,
                forbidden("You don't have permission to create words in the target language"),
            )
            .await;
        }

        let paths = attempt!(s, scs.find_derivation_paths(language.id).await);
        let path = match paths.into_iter().find(|p| p.language_id == target_language.id) {
            Some(p) => p,
            None => {
                return render_generic_error(
                    s,
                    not_found("No complete sound change path found to that language"),
                )
                .await
            }
        };

        // Apply SCS chain to ipa (or word text if no ipa)
        let (input, use_ipa) = if !word.ipa.is_empty() {
            (word.ipa.clone(), true)
        } else {
            (word.word.clone(), false)
        };

        let mut current = input;
        for scs_id in &path.scs_ids {
            let resp = attempt!(s, scs.run_from_db(scs_id, vec![current]).await);
            current = resp.output_words.into_iter().next().unwrap_or_default();
        }

        let derived_ipa = if use_ipa { current.clone() } else { String::new() };
        let derived_word = if use_ipa { String::new() } else { current.clone() };

        // Try to match word class by abbreviation in target language
        let word_class_abbrev = if let Some(abbrev) = &word.word_class_abbreviation {
            word_classes
                .find_by_abbreviation(&target_language.id, abbrev)
                .await
                .ok()
                .map(|wc| wc.abbreviation)
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Load source word's definitions
        let defs = definitions_repo
            .list_by_word(word.id, PaginatedRequest { limit: 100, offset: 0 })
            .await
            .map(|r| r.items)
            .unwrap_or_default();

        // Build redirect query string
        let mut parts: Vec<String> = Vec::new();
        if !derived_ipa.is_empty() {
            parts.push(format!("ipa={}", encode_query_value(&derived_ipa)));
        }
        if !derived_word.is_empty() {
            parts.push(format!("word={}", encode_query_value(&derived_word)));
        }
        if !word_class_abbrev.is_empty() {
            parts.push(format!(
                "word_class={}",
                encode_query_value(&word_class_abbrev)
            ));
        }
        for def in &defs {
            parts.push(format!(
                "definitions%5B%5D={}",
                encode_query_value(&def.definition)
            ));
            parts.push(format!(
                "contexts%5B%5D={}",
                encode_query_value(&def.context)
            ));
        }
        parts.push(format!(
            "antecedent_bookmark={}",
            encode_query_value(&word.bookmark)
        ));
        parts.push("relation_kind=descendant".to_string());

        let redirect_url = format!(
            "/languages/{}/new-word?{}",
            target_language.code,
            parts.join("&")
        );
        return (
            StatusCode::SEE_OTHER,
            Redirect::to(&redirect_url).into_response(),
        );
    }

    // Show step 1 page
    let derivation_paths = attempt!(s, scs.find_derivation_paths(language.id).await);
    let family_infos = attempt!(s, scs.find_family_language_infos(language.id).await);

    let single_family = {
        let family_ids: std::collections::HashSet<Uuid> =
            family_infos.iter().map(|fi| fi.family_id).collect();
        family_ids.len() <= 1
    };

    let descendants: Vec<DescendantOption> = derivation_paths
        .into_iter()
        .filter_map(|path| {
            let fi = family_infos
                .iter()
                .find(|fi| fi.language_id == path.language_id)?;
            Some(DescendantOption {
                language_code: fi.language_code.clone(),
                language_name: fi.language_name.clone(),
                family_name: fi.family_name.clone(),
            })
        })
        .collect();

    let mut loan_options =
        attempt!(s, languages.list_editable_by_user(current_user.id).await);
    loan_options.retain(|l| l.id != language.id);

    let template = DeriveIntoFamilyTemplate {
        current_user: Some(current_user),
        language,
        word,
        descendants,
        loan_options,
        single_family,
        error: None,
    };

    let body = render_template(template);
    okay(body)
}

#[allow(clippy::too_many_arguments)]
async fn derive_into_family_loan(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    language_permissions: LanguagePermissionRepository,
    definitions_repo: DefinitionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Form(form): Form<DeriveIntoFamilyLoanForm>,
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

    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    if !has_permission {
        return render_generic_error(
            s,
            forbidden("You don't have permission to derive words from this language"),
        )
        .await;
    }

    let target_language = attempt!(s, languages.find_by_code(&form.target_language_code).await);

    let has_target_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, target_language.id, PermissionLevel::Editor)
                .await
        );
    if !has_target_permission {
        return render_generic_error(
            s,
            forbidden("You don't have permission to create words in the target language"),
        )
        .await;
    }

    // Try to match word class by abbreviation in target language
    let word_class_abbrev = if let Some(abbrev) = &word.word_class_abbreviation {
        word_classes
            .find_by_abbreviation(&target_language.id, abbrev)
            .await
            .ok()
            .map(|wc| wc.abbreviation)
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Load source word's definitions
    let defs = definitions_repo
        .list_by_word(word.id, PaginatedRequest { limit: 100, offset: 0 })
        .await
        .map(|r| r.items)
        .unwrap_or_default();

    // Build redirect query string (loan: copy word + ipa as-is)
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("word={}", encode_query_value(&word.word)));
    if !word.ipa.is_empty() {
        parts.push(format!("ipa={}", encode_query_value(&word.ipa)));
    }
    if !word_class_abbrev.is_empty() {
        parts.push(format!(
            "word_class={}",
            encode_query_value(&word_class_abbrev)
        ));
    }
    for def in &defs {
        parts.push(format!(
            "definitions%5B%5D={}",
            encode_query_value(&def.definition)
        ));
        parts.push(format!(
            "contexts%5B%5D={}",
            encode_query_value(&def.context)
        ));
    }
    parts.push(format!(
        "antecedent_bookmark={}",
        encode_query_value(&word.bookmark)
    ));
    parts.push(format!(
        "relation_kind={}",
        encode_query_value(&form.relation_kind.to_string())
    ));

    let redirect_url = format!(
        "/languages/{}/new-word?{}",
        target_language.code,
        parts.join("&")
    );
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&redirect_url).into_response(),
    )
}

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/languages/{language}/new-word", axum::routing::get(new_word))
        .route("/languages/{language}/words/{slug}/{lemma}/add-relation", axum::routing::post(add_relation_submit))
        .route("/languages/{language}/new-word", axum::routing::post(new_word_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/edit", axum::routing::get(edit_word))
        .route("/languages/{language}/words/{slug}/{lemma}/edit", axum::routing::post(edit_word_submit))
        .route("/languages/{language}/new-word/estimate-ipa", axum::routing::post(estimate_ipa_new_word))
        .route("/languages/{language}/words/{slug}/{lemma}/estimate-ipa", axum::routing::post(estimate_ipa_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/delete", axum::routing::post(delete_word_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/delete", axum::routing::post(delete_relation_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/edit", axum::routing::post(edit_relation_submit))
        .route("/languages/{language}/words/{slug}/{lemma}/derive-into-family", axum::routing::post(derive_into_family_loan));

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{language}/words", axum::routing::get(word_search))
        .route("/languages/{language}/words/{slug}", axum::routing::get(view_lemmata))
        .route("/languages/{language}/words/{slug}/{lemma}", axum::routing::get(view_lemma))
        .route("/languages/{language}/words/{slug}/{lemma}/relations", axum::routing::get(view_word_relations))
        .route("/languages/{language}/words/{slug}/{lemma}/add-relation", axum::routing::get(add_relation_form))
        .route("/languages/{language}/words/{slug}/{lemma}/delete", axum::routing::get(delete_word_form))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/delete", axum::routing::get(delete_relation_form))
        .route("/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/edit", axum::routing::get(edit_relation_form))
        .route("/languages/{language}/words/{slug}/{lemma}/derive-into-family", axum::routing::get(derive_into_family_form));

    (secure_routes, normal_routes)
}
