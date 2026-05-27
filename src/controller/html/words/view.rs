use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use futures::TryFutureExt;

use super::search::{WordSearchOptions, build_categories_json};
use crate::{
    attempt,
    controller::html::{self, okay, render_generic_error, render_template, words},
    embed::{EmbedTarget, render_embed},
    err::not_found,
    model::{
        definitions::{Definition, DefinitionRepository},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        quotations::{QuotationRepository, QuotationWithSpan},
        users::{User, UserRepository},
        word_categories::{WordCategory, WordCategoryRepository},
        word_classes::WordClassRepository,
        word_relations::{SearchWordRelations, WordRelationRepository, WordRelationSearchResult},
        words::{Word, WordRepository, WordSearch, WordWithMeta},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{
        AppState, BackQuery, ListHeaderKind,
        extract_session::Session,
        graph_svg::cognacy_to_svg,
        is_discord,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

#[derive(Template)]
#[template(path = "words/lemma.html")]
#[allow(dead_code)]
struct LemmaTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    word_class: Option<crate::model::word_classes::WordClass>,
    word_categories: Vec<WordCategory>,
    // definition, quotations, has_more
    definitions: Vec<(Definition, Vec<QuotationWithSpan>, bool)>,
    other_lemmata: bool,
    back: String,
    user_has_permission: bool,
    rendered_notes: String,
    creator: User,
    updater: Option<User>,
    contributor_count: i64,
    is_liked: bool,
    recent_relations: Vec<WordRelationSearchResult>,
    total_relations: i64,
    cognacy: Option<Result<String, String>>,
    non_cognacy_relations: Vec<WordRelationSearchResult>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn view_lemmata(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug)): Path<(String, String)>,
    axum_extra::extract::Query(mut query): axum_extra::extract::Query<WordSearch>,
    axum_extra::extract::Query(pagination): axum_extra::extract::Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    query.exact_slug = Some(slug.clone());

    let user_applied_filters = query.q.is_some()
        || query.word_class.is_some()
        || query.created_before.is_some()
        || query.created_after.is_some()
        || !query.categories.is_empty();

    // Shortcut: when the user hasn't applied any filters and this slug has
    // exactly one lemma, skip the search page and go to the lemma's full view.
    if !user_applied_filters {
        match attempt!(s, words.count_by_slug(language.id, &slug).await) {
            0 => {
                return render_generic_error(s, not_found(format!("word with slug '{slug}'")))
                    .await;
            }
            1 => {
                let only = attempt!(
                    s,
                    words
                        .search(
                            &language.id,
                            PaginatedRequest {
                                limit: 1,
                                offset: 0,
                            },
                            query.clone(),
                        )
                        .await
                );
                if let Some(lemma) = only.items.first() {
                    return (
                        StatusCode::SEE_OTHER,
                        Redirect::to(&format!(
                            "/languages/{}/words/{}/{}",
                            language_code, slug, lemma.lemma
                        ))
                        .into_response(),
                    );
                }
            }
            _ => {}
        }
    }

    let search_action = format!("/languages/{}/words/{}", language.code, slug);
    let back_url = crate::util::back_url(&search_action, &pagination, &query);

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
    let word_categories_list = attempt!(s, word_categories.list_all(language.id).await);
    let word_categories_json = build_categories_json(&word_categories_list);
    let selected_category_abbrevs = query.categories.clone();
    let query_template = WordSearchOptions {
        query: query.clone(),
        word_classes: word_classes_list,
        word_categories: word_categories_list,
        word_categories_json,
        selected_category_abbrevs,
    };

    let breadcrumbs = html::languages::Breadcrumb {
        language: &language,
    };

    let results = words
        .search(&language.id, pagination.clone(), query.clone())
        .and_then(|results| results.try_map_async(|word| words.materialize(word, s.user())))
        .await;

    let render_item = |word_with_meta: &WordWithMeta| words::PreviewCard {
        word_with_meta: word_with_meta.clone(),
        back_url: &back_url,
    };

    let header = words::Header {
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
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();

    (status, render_template(template))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn view_lemma(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    definitions_repo: DefinitionRepository,
    word_relations: WordRelationRepository,
    word_classes: WordClassRepository,
    word_categories_repo: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    quotations: QuotationRepository,
    users: UserRepository,
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

    let word_class = if let Some(word_class_id) = word.word_class {
        Some(attempt!(s, word_classes.find_by_id(word_class_id).await))
    } else {
        None
    };

    let word_categories = attempt!(s, word_categories_repo.list_by_word(word.id, None).await);

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
            .and_then(async |res: PaginatedResponse<Definition>| {
                res.try_map_async(async |d| {
                    let quotations = quotations
                        .search_by_definition(
                            d.clone(),
                            Default::default(),
                            PaginatedRequest {
                                limit: 5,
                                offset: 0,
                            },
                        )
                        .await?;
                    let has_more = quotations.total > 5;
                    Ok((d, quotations.items, has_more))
                })
                .await
            })
            .await
            .map(|defs| (defs.items, defs.has_more))
    );

    let other_lemmata = attempt!(s, words.count_by_slug(language.id, &slug).await) > 1;

    let back = params.back.unwrap_or_default();

    let rendered_notes = attempt!(s, WordRepository::render_notes(&word));

    let creator = attempt!(s, words.find_creator(&word.id).await);

    let updater = match (word._updated_by, word._created_by) {
        (Some(updated_by), Some(created_by)) if updated_by != created_by => {
            Some(attempt!(s, users.find_by_id(updated_by).await))
        }
        _ => None,
    };

    let contributor_count = attempt!(s, words.count_contributors(word.id).await);

    let is_liked = if let Some(user) = &current_user {
        words.is_liked(&word.id, &user.id).await.unwrap_or(false)
    } else {
        false
    };

    let cognacy = attempt!(s, word_relations.get_leveled_cognacy(&word).await);
    let cognacy = cognacy.map(|c| {
        cognacy_to_svg(&c, Some(word.id)).map_err(|e| {
            tracing::error!(
                word_id = %word.id,
                error = %e.message,
                "failed to render cognacy graph",
            );
            e.message
        })
    });

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

    let non_cognacy_relations = attempt!(
        s,
        word_relations
            .search(
                PaginatedRequest::preview(),
                SearchWordRelations {
                    non_cognacy_relations_only: Some(true),
                    ..Default::default()
                },
                &word
            )
            .await
            .map(|res| res.items)
    );

    let template = LemmaTemplate {
        current_user,
        language,
        word,
        word_class,
        word_categories,
        definitions,
        other_lemmata,
        back,
        user_has_permission,
        rendered_notes,
        creator,
        updater,
        contributor_count,
        is_liked,
        recent_relations,
        total_relations,
        cognacy,
        non_cognacy_relations,
    };

    let body = render_template(template);
    okay(body)
}
