use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use futures::TryFutureExt;

use crate::{
    attempt,
    controller::html::{self, render_template, words},
    model::{
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        word_classes::{WordClass, WordClassRepository},
        words::{WordRepository, WordSearch, WordWithMeta},
    },
    pagination::PaginatedRequest,
    util::{
        AppState, ListHeaderKind,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};
#[derive(Template)]
#[template(path = "words/fragments/query.html")]
struct WordSearchOptions {
    query: WordSearch,
    word_classes: Vec<WordClass>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn word_search(
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
    let query_template = WordSearchOptions {
        query: query.clone(),
        word_classes: word_classes_list,
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
