use askama::Template;
use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    err::{forbidden, not_found},
    get_user,
    model::{
        definitions::DefinitionRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        sound_change_sets::SoundChangeSetRepository,
        users::User,
        word_classes::WordClassRepository,
        word_relations::WordRelationType,
        words::{Word, WordRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session},
};

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
    error: Option<crate::err::AppError>,
}

struct DescendantOption {
    language_code: String,
    language_name: String,
    family_name: String,
}

#[derive(Deserialize, Default)]
pub(super) struct DeriveQuery {
    descendant: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct DeriveIntoFamilyLoanForm {
    target_language_code: String,
    relation_kind: WordRelationType,
}

fn encode_query_value(s: &str) -> String {
    percent_encoding::percent_encode(s.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn derive_into_family_form(
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
        let path = match paths
            .into_iter()
            .find(|p| p.language_id == target_language.id)
        {
            Some(p) => p,
            None => {
                return render_generic_error(
                    s,
                    not_found("No complete sound change path found to that language"),
                )
                .await;
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

        let derived_ipa = if use_ipa {
            current.clone()
        } else {
            String::new()
        };
        let derived_word = if use_ipa {
            String::new()
        } else {
            current.clone()
        };

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
            .list_by_word(
                word.id,
                PaginatedRequest {
                    limit: 100,
                    offset: 0,
                },
            )
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

    let mut loan_options = attempt!(s, languages.list_editable_by_user(current_user.id).await);
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
pub(super) async fn derive_into_family_loan(
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
        .list_by_word(
            word.id,
            PaginatedRequest {
                limit: 100,
                offset: 0,
            },
        )
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
