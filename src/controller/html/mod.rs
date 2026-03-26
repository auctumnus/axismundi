use std::sync::Arc;

use crate::{
    ErrorTemplate,
    embed::{self},
    err::{AppError, internal_error},
    model::{
        contribution_stats::ContributionStatsRepository,
        language_families::{FamilyWithContributors, LanguageFamilyRepository},
        languages::{Language, LanguageRepository},
        translatable::{TranslatableRepository, TranslatableSearch, TranslatableWithLiked},
        user_activities::{UserActivity, UserActivityRepository},
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session},
};

use crate::attempt;
use askama::Template;
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use governor::middleware::NoOpMiddleware;
use serde::Serialize;
use tower_governor::governor::GovernorConfig;
use tower_http::services::ServeDir;

mod audit_logs;
mod bookmarks;
mod language_families;
mod language_family_invites;
mod language_family_members;
mod language_family_permissions;
mod language_invites;
mod language_permissions;
mod languages;
mod reports;
mod translatables;
mod translations;
mod user_bans;
mod users;
mod phonology_tables;
mod sound_change_sets;
mod word_classes;
mod words;

pub fn create_html_controller() -> Router<AppState> {
    let secure_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::secure());
    let normal_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::default());

    let secure_limiter = secure_governor.limiter().clone();
    let normal_limiter = normal_governor.limiter().clone();
    let interval = std::time::Duration::from_secs(60);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            secure_limiter.retain_recent();
            normal_limiter.retain_recent();
        }
    });

    let (secure_user_routes, normal_user_routes) = users::create_router();
    let (secure_language_routes, normal_language_routes) = languages::create_router();
    let (secure_language_family_routes, normal_language_family_routes) =
        language_families::create_router();
    let (secure_language_permission_routes, normal_language_permission_routes) =
        language_permissions::create_router();
    let (secure_family_invite_routes, normal_family_invite_routes) =
        language_family_invites::create_router();
    let (secure_family_permission_routes, normal_family_permission_routes) =
        language_family_permissions::create_router();
    let (secure_word_routes, normal_word_routes) = words::create_router();
    let (secure_word_class_routes, normal_word_class_routes) = word_classes::create_router();
    let (secure_translatable_routes, normal_translatable_routes) = translatables::create_router();
    let (secure_translation_routes, normal_translation_routes) = translations::create_router();
    let (secure_bookmark_routes, normal_bookmark_routes) = bookmarks::create_router();
    let (secure_invite_routes, normal_invite_routes) = language_invites::create_router();
    let (secure_audit_log_routes, normal_audit_log_routes) = audit_logs::create_router();
    let (secure_ban_routes, normal_ban_routes) = user_bans::create_router();
    let (secure_report_routes, normal_report_routes) = reports::create_router();
    let (secure_phonology_table_routes, normal_phonology_table_routes) =
        phonology_tables::create_router();
    let (secure_sound_change_set_routes, normal_sound_change_set_routes) =
        sound_change_sets::create_router();

    let secure_routes = Router::<AppState>::new()
        .merge(secure_user_routes)
        .merge(secure_language_routes)
        .merge(secure_language_family_routes)
        .merge(secure_language_permission_routes)
        .merge(secure_family_invite_routes)
        .merge(secure_family_permission_routes)
        .merge(language_family_members::create_router())
        .merge(secure_word_routes)
        .merge(secure_word_class_routes)
        .merge(secure_translatable_routes)
        .merge(secure_translation_routes)
        .merge(secure_bookmark_routes)
        .merge(secure_invite_routes)
        .merge(secure_audit_log_routes)
        .merge(secure_ban_routes)
        .merge(secure_report_routes)
        .merge(secure_phonology_table_routes)
        .merge(secure_sound_change_set_routes);

    let normal_routes = Router::<AppState>::new()
        .route("/", get(landing))
        .route("/home", get(home))
        .route("/services/oembed", axum::routing::get(oembed))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("assets"))
        .merge(normal_user_routes)
        .merge(normal_language_routes)
        .merge(normal_language_family_routes)
        .merge(normal_language_permission_routes)
        .merge(normal_family_invite_routes)
        .merge(normal_family_permission_routes)
        .merge(normal_word_routes)
        .merge(normal_word_class_routes)
        .merge(normal_translatable_routes)
        .merge(normal_translation_routes)
        .merge(normal_bookmark_routes)
        .merge(normal_invite_routes)
        .merge(normal_audit_log_routes)
        .merge(normal_ban_routes)
        .merge(normal_report_routes)
        .merge(normal_phonology_table_routes)
        .merge(normal_sound_change_set_routes);

    Router::<AppState>::new()
        .merge(secure_routes)
        .merge(normal_routes)
}

fn render_template<T: Template>(template: T) -> Response {
    template
        .render()
        .map_or_else(
            |e| {
                tracing::error!("Template rendering error: {}", e);
                Html("500 Internal Server Error".to_string())
            },
            Html,
        )
        .into_response()
}

#[derive(Template)]
#[template(path = "landing.html")]
struct LandingTemplate {
    current_user: Option<User>,
}

async fn landing(s: Session) -> impl IntoResponse {
    if let Some(_user) = s.user() {
        return Redirect::to("/home").into_response();
    }

    let current_user = s.user().cloned();
    render_template(LandingTemplate { current_user })
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguagesWithContributors {
    pub language: Language,
    pub top_contributors: Vec<User>,
    #[allow(dead_code)]
    pub is_liked: bool,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    #[allow(dead_code)]
    error: Option<AppError>,
    current_user: Option<User>,
    languages: Vec<LanguagesWithContributors>,
    families: Vec<FamilyWithContributors>,
    activities: Vec<UserActivity>,
    translatables: Vec<TranslatableWithLiked>,
}

async fn home(
    users: UserRepository,
    languages: LanguageRepository,
    families_repo: LanguageFamilyRepository,
    translatables: TranslatableRepository,
    activities_repo: UserActivityRepository,
    contribution_stats: ContributionStatsRepository,
    s: Session,
) -> (StatusCode, Response) {
    let Some(user) = s.user().cloned() else {
        return (StatusCode::OK, Redirect::to("/").into_response());
    };

    let Ok(l) = users.top_languages(user.id, 5).await else {
        return render_generic_error(s, internal_error("Failed to load top languages")).await;
    };
    let mut languages_with_contributors = Vec::with_capacity(l.len());
    for lang in &l {
        let top_contributors = attempt!(
            s,
            contribution_stats.get_top_contributors(&lang.id, 5).await
        );
        let is_liked = attempt!(s, languages.is_liked(&user.id, &lang.id).await);
        let l = LanguagesWithContributors {
            language: lang.clone(),
            top_contributors,
            is_liked,
        };
        languages_with_contributors.push(l);
    }
    let languages = languages_with_contributors;

    let Ok(f) = families_repo.top_families(user.id, 3).await else {
        return render_generic_error(s, internal_error("Failed to load top families")).await;
    };
    let mut families = Vec::with_capacity(f.len());
    for family in &f {
        let materialized = attempt!(
            s,
            families_repo.materialize(family.clone(), Some(&user)).await
        );
        families.push(materialized);
    }

    let translatables_res = translatables
        .search(
            PaginatedRequest {
                limit: 5,
                offset: 0,
            },
            TranslatableSearch::default(),
        )
        .await
        .map_or_else(|_| vec![], |res| res.items);

    let translatables_with_liked = {
        let mut vec = Vec::with_capacity(translatables_res.len());
        for t in translatables_res {
            let is_liked = attempt!(s, translatables.is_liked(&user.id, &t.id).await);
            vec.push(TranslatableWithLiked {
                translatable: t,
                is_liked,
            });
        }
        vec
    };

    let Ok(activities) = activities_repo.list_site_wide(s.user()).await else {
        return render_generic_error(s, internal_error("Failed to load user activities")).await;
    };

    let template = HomeTemplate {
        current_user: Some(user),
        languages,
        families,
        activities,
        translatables: translatables_with_liked,
        error: None,
    };

    let body = render_template(template);
    (StatusCode::OK, body)
}

async fn oembed(
    State(state): State<AppState>,
    Query(request): Query<embed::OEmbedRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    println!("sdbgffdskljgbfdgksj");
    let response = embed::get_oembed(state, &request).await?;

    Ok(axum::Json(response))
}

pub async fn render_generic_error(s: Session, error: AppError) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let status_code = error.status_code;
    let template = ErrorTemplate {
        current_user,
        error,
    };
    let body = render_template(template);
    (status_code, body)
}

pub fn no_session(redirect: Option<String>) -> (StatusCode, Response) {
    match redirect {
        Some(redirect) => {
            let login_url = format!(
                "/login?redirect={}",
                percent_encoding::percent_encode(
                    redirect.as_bytes(),
                    percent_encoding::NON_ALPHANUMERIC
                )
            );
            (
                StatusCode::TEMPORARY_REDIRECT,
                Redirect::to(&login_url).into_response(),
            )
        }
        None => (
            StatusCode::TEMPORARY_REDIRECT,
            Redirect::to("/login").into_response(),
        ),
    }
}

pub fn okay(res: Response) -> (StatusCode, Response) {
    (StatusCode::OK, res)
}

#[macro_use]
mod macros {
    #[macro_export]
    macro_rules! attempt {
        ($session:expr, $expr:expr) => {
            match $expr {
                Ok(val) => val,
                Err(e) => return $crate::controller::html::render_generic_error($session, e).await,
            }
        };
    }

    #[macro_export]
    macro_rules! get_user {
        ($session:expr) => {
            match $session.user().cloned() {
                Some(user) => user,
                None => {
                    return $crate::controller::html::no_session(None);
                }
            }
        };
        ($session:expr, $redirect:expr) => {
            match $session.user().cloned() {
                Some(user) => user,
                None => {
                    return $crate::controller::html::no_session(Some($redirect));
                }
            }
        };
    }
}
