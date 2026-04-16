use askama::Template;
use axum::{
    Form,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use futures::TryFutureExt;
use itertools::Itertools;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_template, words::create::NewWordPrefill},
    err::{AppError, internal_error, not_found},
    get_user,
    model::{
        definitions::DefinitionRepository,
        language_families::{LanguageFamily, LanguageFamilyRepository, SearchLanguageFamilies},
        languages::{Language, LanguageRepository},
        sound_change_sets::{DerivationPath, SoundChangeSetRepository},
        users::User,
        word_classes::WordClassRepository,
        word_relations::WordRelationType,
        words::{WordRepository, WordWithMeta},
    },
    pagination::PaginatedRequest,
    util::extract_session::Session,
};

#[derive(Template)]
#[template(path = "words/derive_into_family.html")]
#[allow(dead_code)]
struct DeriveIntoFamilyTemplate {
    current_user: Option<User>,
    language: Language,
    selected_language_id: Option<Uuid>,
    primary_language_family: Option<LanguageFamily>,
    word: WordWithMeta,
    families: Vec<(String, Vec<DerivationPath>)>,
    error: Option<AppError>,
    other_lemmata: bool,
    goes_from_word: bool,
}

#[allow(dead_code)]
struct DescendantOption {
    language_code: String,
    language_name: String,
    family_name: String,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
pub(super) struct DeriveQuery {
    descendant: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(super) struct DeriveIntoFamilyLoanForm {
    target_language_code: String,
    relation_kind: WordRelationType,
}

pub(super) async fn derive_form(
    s: Session,
    languages: LanguageRepository,
    language_families: LanguageFamilyRepository,
    words: WordRepository,
    scs: SoundChangeSetRepository,
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

    let paths = attempt!(
        s,
        scs.find_derivation_paths(&current_user, language.id).await
    );

    let paths = paths
        .into_iter()
        .chunk_by(|p| p.family_name.clone())
        .into_iter()
        .map(|(group, paths)| {
            let family_name = group.to_string();
            let mut derivation_paths = paths.collect::<Vec<_>>();
            derivation_paths.sort_by(|a, b| {
                b.has_direct_permission
                    .cmp(&a.has_direct_permission)
                    .then_with(|| a.language_name.cmp(&b.language_name))
            });
            (family_name, derivation_paths)
        })
        .collect::<Vec<_>>();

    let primary_language_family = attempt!(
        s,
        language_families
            .search(
                SearchLanguageFamilies {
                    has_language: Some(language.code.clone()),
                    ..Default::default()
                },
                PaginatedRequest {
                    limit: 1,
                    offset: 0
                }
            )
            .await
    )
    .items
    .into_iter()
    .next();

    let template = DeriveIntoFamilyTemplate {
        current_user: Some(current_user),
        language,
        goes_from_word: word.word.ipa.is_empty(),
        word,
        families: paths,
        error: None,
        other_lemmata,
        primary_language_family,
        selected_language_id: None,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
pub(super) struct DeriveForm {
    derivation_target: Uuid,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn derive_submit(
    s: Session,
    languages: LanguageRepository,
    language_families: LanguageFamilyRepository,
    word_classes: WordClassRepository,
    words: WordRepository,
    scs: SoundChangeSetRepository,
    definitions: DefinitionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Form(form): Form<DeriveForm>,
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

    let paths = attempt!(
        s,
        scs.find_derivation_paths(&current_user, language.id).await
    );

    let path = paths
        .iter()
        .find(|p| p.language_id == form.derivation_target)
        .cloned();

    let render_error = async |error: AppError| {
        let other_lemmata = words.count_by_slug(language.id, &slug).await? > 1;

        let paths = paths
            .into_iter()
            .chunk_by(|p| p.family_name.clone())
            .into_iter()
            .map(|(group, paths)| {
                let family_name = group.to_string();
                let mut derivation_paths = paths.collect::<Vec<_>>();
                derivation_paths.sort_by(|a, b| {
                    b.has_direct_permission
                        .cmp(&a.has_direct_permission)
                        .then_with(|| a.language_name.cmp(&b.language_name))
                });
                (family_name, derivation_paths)
            })
            .collect::<Vec<_>>();

        let primary_language_family = language_families
            .search(
                SearchLanguageFamilies {
                    has_language: Some(language.code.clone()),
                    ..Default::default()
                },
                PaginatedRequest {
                    limit: 1,
                    offset: 0,
                },
            )
            .await?
            .items
            .into_iter()
            .next();

        Ok(DeriveIntoFamilyTemplate {
            current_user: Some(current_user),
            language: language.clone(),
            word: word.clone(),
            goes_from_word: word.word.ipa.is_empty(),
            families: paths,
            error: Some(error),
            other_lemmata,
            primary_language_family,
            selected_language_id: form.derivation_target.into(),
        })
    };

    let path = attempt!(
        render_error,
        s,
        path.ok_or(not_found("no derivation path from this language"))
    );

    let target_language = attempt!(
        render_error,
        s,
        languages.find_by_id(path.language_id).await
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

    let (input_word, is_from_ipa) = if word.word.ipa.is_empty() {
        (word.word.word.clone(), false)
    } else {
        (word.word.ipa.clone(), true)
    };

    let input_words = vec![input_word];

    let response = attempt!(
        render_error,
        s,
        scs.run_derivation_path(path.scs_ids, input_words).await
    );

    let response = attempt!(
        render_error,
        s,
        response.ok_or(internal_error("derivation path of 0 length"))
    );

    let derived_word =
        attempt!(
            render_error,
            s,
            response.output_words.into_iter().next().ok_or_else(|| {
                internal_error("derivation path did not produce any output words")
            })
        );

    let (derived_word, derived_ipa) = if is_from_ipa {
        (String::new(), derived_word)
    } else {
        (derived_word, String::new())
    };

    let prefill = NewWordPrefill {
        word: Some(derived_word),
        ipa: Some(derived_ipa),
        word_class,
        definitions,
        contexts,
        antecedent_bookmark: Some(word.word.bookmark.clone()),
        relation_kind: Some(WordRelationType::Derived),
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
