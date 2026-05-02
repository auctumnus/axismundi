use askama::Template;
use axum::{
    Form,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use futures::TryFutureExt;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{okay, render_template, words::create::NewWordPrefill},
    err::AppError,
    get_user,
    model::{
        definitions::DefinitionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_classes::WordClassRepository,
        word_relations::WordRelationType,
        words::{WordRepository, WordWithMeta},
    },
    pagination::PaginatedRequest,
    util::extract_session::Session,
};

#[derive(Template)]
#[template(path = "words/loan.html")]
#[allow(dead_code)]
struct LoanTemplate {
    current_user: Option<User>,
    language: Language,
    word: WordWithMeta,
    languages: Vec<Language>,
    previous_language_code: String,
    goes_from_word: bool,
    error: Option<AppError>,
    other_lemmata: bool,
}

pub(super) async fn loan_form(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .and_then(|word| words.materialize(word, Some(&current_user)))
            .await
    );
    let other_lemmata = attempt!(s, words.count_by_slug(language.id, &slug).await) > 1;

    let editable_languages = attempt!(s, languages.list_editable_by_user(current_user.id).await);

    let template = LoanTemplate {
        goes_from_word: word.word.ipa.is_empty(),
        word,
        language,
        languages: editable_languages,
        previous_language_code: String::new(),
        error: None,
        other_lemmata,
        current_user: Some(current_user),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
pub(super) struct LoanForm {
    loan_target: String,
}

pub(super) async fn loan_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    definitions: DefinitionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Form(form): Form<LoanForm>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .and_then(|word| words.materialize(word, Some(&current_user)))
            .await
    );

    let editable_languages = attempt!(s, languages.list_editable_by_user(current_user.id).await);

    let render_error = async |error: AppError| {
        let other_lemmata = words.count_by_slug(language.id, &slug).await? > 1;
        Ok(LoanTemplate {
            current_user: Some(current_user),
            language: language.clone(),
            goes_from_word: word.word.ipa.is_empty(),
            word: word.clone(),
            languages: editable_languages.clone(),
            previous_language_code: form.loan_target.clone(),
            error: Some(error),
            other_lemmata,
        })
    };

    let target_language = attempt!(
        render_error,
        s,
        languages.find_by_code(&form.loan_target).await
    );

    let (definitions, contexts) = attempt!(
        render_error,
        s,
        definitions
            .list_by_word(
                word.word.id,
                PaginatedRequest {
                    limit: 100,
                    offset: 0
                }
            )
            .await
            .map(|r| r
                .items
                .into_iter()
                .map(|d| (d.definition, d.context))
                .unzip())
    );

    let word_class = if let Some(abbrev) = &word.word.word_class_abbreviation {
        match word_classes
            .find_by_abbreviation(&language.id, abbrev)
            .await
        {
            Ok(wc) => Some(wc.abbreviation),
            Err(e) => {
                let status_code = e.status_code;
                if status_code == StatusCode::NOT_FOUND {
                    None
                } else {
                    return attempt!(
                        s,
                        render_error(e)
                            .await
                            .map(|t| (status_code, render_template(t)))
                    );
                }
            }
        }
    } else {
        None
    };

    let (loaned_word, loaned_ipa) = if word.word.ipa.is_empty() {
        (Some(word.word.word.clone()), None)
    } else {
        (None, Some(word.word.ipa.clone()))
    };

    let prefill = NewWordPrefill {
        word: loaned_word,
        ipa: loaned_ipa,
        word_class,
        definitions,
        contexts,
        categories: Vec::new(),
        antecedent_bookmark: Some(word.word.bookmark.clone()),
        relation_kind: Some(WordRelationType::Borrowed),
    };

    let query_string = attempt!(
        render_error,
        s,
        serde_html_form::to_string(&prefill).map_err(Into::<AppError>::into)
    );

    let redirect_url = format!(
        "/languages/{}/new-word?{}",
        target_language.code, query_string
    );

    (
        StatusCode::SEE_OTHER,
        Redirect::to(&redirect_url).into_response(),
    )
}
