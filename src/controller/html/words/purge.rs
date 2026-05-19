use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    get_user,
    model::{
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        words::WordRepository,
    },
    util::{AppState, extract_session::Session},
};

#[derive(Template)]
#[template(path = "words/purge.html")]
#[allow(dead_code)]
struct PurgeDictionaryTemplate {
    current_user: Option<User>,
    language: Language,
    word_count: i64,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
}

pub(super) async fn purge_dictionary_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    permissions: LanguagePermissionRepository,
    Path(language_code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    let can_edit_language = attempt!(
        s,
        permissions
            .can_edit_language(Some(&user), &language.id)
            .await
    );
    if !can_edit_language {
        return render_generic_error(
            s,
            crate::err::forbidden("you don't have permission to purge this dictionary"),
        )
        .await;
    }
    let can_delete_language = attempt!(
        s,
        permissions
            .can_delete_language(Some(&user), &language.id)
            .await
    );

    let word_count = attempt!(s, words.count_by_language(language.id).await);
    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = PurgeDictionaryTemplate {
        current_user: Some(user),
        language,
        word_count,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
    };

    okay(render_template(template))
}

pub(super) async fn purge_dictionary_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path(language_code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    match words.purge_dictionary(&user, language.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{language_code}")).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
