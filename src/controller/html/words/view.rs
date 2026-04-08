use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::{TypedHeader, headers::UserAgent};

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    embed::{EmbedTarget, render_embed},
    err::not_found,
    model::{
        definitions::{Definition, DefinitionRepository},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_classes::WordClassRepository,
        word_relations::{SearchWordRelations, WordRelationRepository, WordRelationSearchResult},
        words::{Word, WordRepository, WordSearch},
    },
    pagination::PaginatedRequest,
    util::{AppState, BackQuery, extract_session::Session, is_discord},
};

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

#[allow(clippy::too_many_arguments)]
pub(super) async fn view_lemmata(
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
pub(super) async fn view_lemma(
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

    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(current_user.as_ref(), language.id, &slug, lemma)
            .await
    );

    if is_discord(user_agent) {
        let embed = attempt!(s, words.as_embed(&word).await);
        return okay(
            render_embed(EmbedTarget::Discord, embed)
                .await
                .into_response(),
        );
    }

    let user_has_permission = attempt!(
        s,
        permissions
            .can_edit_language(current_user.as_ref(), &language.id)
            .await
    );

    // Fetch definitions for this word
    let (definitions, _has_more) = attempt!(
        s,
        definitions_repo
            .list_by_word(
                word.id,
                PaginatedRequest {
                    limit: 100,
                    offset: 0,
                },
            )
            .await
            .map(|res| (res.items, res.total))
    );

    let other_lemmata = attempt!(s, words.count_by_slug(language.id, &slug).await) > 1;

    let back = params.back.unwrap_or_default();

    let rendered_notes = attempt!(s, WordRepository::render_notes(&word));

    let creator = attempt!(s, words.find_creator(&word.id).await);

    let contributor_count = attempt!(s, words.count_contributors(word.id).await);

    let is_liked = if let Some(user) = &current_user {
        words.is_liked(&word.id, &user.id).await.unwrap_or(false)
    } else {
        false
    };

    // Fetch recent word relations (3 most recent, with cognacy relations first)
    let (recent_relations, total_relations) = attempt!(
        s,
        word_relations
            .search(
                PaginatedRequest::preview(),
                SearchWordRelations::default(),
                &word
            )
            .await
            .map(|res| (res.items, res.total))
    );

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
