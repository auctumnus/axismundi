use askama::Template;
use axum::{
    Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::AppState;
use crate::controller::html::{okay, render_generic_error, render_template};
use crate::err::AppError;
use crate::get_user;
use crate::model::user_bans::{CreateUserBan, UserBanRepository, UserBanSearch};
use crate::model::users::{User, UserRepository};
use crate::pagination::PaginatedRequest;
use crate::util::extract_session::Session;
use axum::Form;

// Form struct that only contains reason - user_id comes from URL
#[derive(Deserialize)]
struct BanUserForm {
    reason: String,
}

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/admin/bans/{username}/ban", post(ban_user_submit))
        .route("/admin/bans/{username}/unban", post(unban_user_submit));

    let normal_routes = Router::new()
        .route("/admin/bans", get(list_bans))
        .route("/admin/bans/{username}/ban", get(ban_user_form))
        .route("/admin/bans/{username}/unban", get(unban_user_form));
    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "user_bans/list.html")]
struct ListBansTemplate {
    current_user: Option<User>,
    bans: Vec<BanWithUsers>,
    previous_query: UserBanSearch,
    pagination: PaginatedRequest,
    has_more: bool,
}

pub struct BanWithUsers {
    pub ban: crate::model::user_bans::UserBan,
    pub banned_user: User,
    pub banned_by_user: User,
}

async fn list_bans(
    s: Session,
    Query(pagination): Query<PaginatedRequest>,
    Query(search): Query<UserBanSearch>,
    user_bans: UserBanRepository,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(&s);

    let response = match user_bans
        .search(&user, pagination.clone(), search.clone())
        .await
    {
        Ok(r) => r,
        Err(e) => return render_generic_error(s, e).await,
    };

    // Fetch user data for each ban
    let mut bans_with_users = Vec::new();
    for ban in response.items {
        let banned_user = match users.find_by_id(ban.user_id).await {
            Ok(u) => u,
            Err(e) => return render_generic_error(s, e).await,
        };

        let banned_by_user = match users.find_by_id(ban.banned_by).await {
            Ok(u) => u,
            Err(e) => return render_generic_error(s, e).await,
        };

        bans_with_users.push(BanWithUsers {
            ban,
            banned_user,
            banned_by_user,
        });
    }

    okay(render_template(ListBansTemplate {
        current_user: Some(user),
        bans: bans_with_users,
        previous_query: search,
        pagination,
        has_more: response.has_more,
    }))
}

#[derive(Template)]
#[template(path = "user_bans/ban.html")]
struct BanTemplate {
    current_user: Option<User>,
    user_to_ban: User,
    error: Option<AppError>,
    previous_reason: Option<String>,
}

async fn ban_user_form(
    s: Session,
    Path(username): Path<String>,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(&s);

    let user_to_ban = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    okay(render_template(BanTemplate {
        current_user: Some(user),
        user_to_ban,
        error: None,
        previous_reason: None,
    }))
}

async fn ban_user_submit(
    s: Session,
    Path(username): Path<String>,
    user_bans: UserBanRepository,
    users: UserRepository,
    Form(form): Form<BanUserForm>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let user_to_ban = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    let previous_reason = form.reason.clone();

    // Construct CreateUserBan with user_id from URL and reason from form
    let req = CreateUserBan {
        user_id: user_to_ban.id,
        reason: form.reason,
    };

    match user_bans.create(&user, req).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            axum::response::Redirect::to("/admin/bans").into_response(),
        ),
        Err(e) => okay(render_template(BanTemplate {
            current_user: Some(user.clone()),
            user_to_ban,
            error: Some(e),
            previous_reason: Some(previous_reason),
        })),
    }
}

#[derive(Template)]
#[template(path = "user_bans/unban.html")]
struct UnbanTemplate {
    current_user: Option<User>,
    ban: crate::model::user_bans::UserBan,
    banned_user: User,
    banned_by_user: User,
    error: Option<AppError>,
}

async fn unban_user_form(
    s: Session,
    Path(username): Path<String>,
    user_bans: UserBanRepository,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(&s);

    let banned_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    let ban = match user_bans.find_by_user_id(banned_user.id).await {
        Ok(Some(b)) => b,
        Ok(None) => {
            return render_generic_error(s, crate::err::not_found("User is not banned")).await;
        }
        Err(e) => return render_generic_error(s, e).await,
    };

    let banned_by_user = match users.find_by_id(ban.banned_by).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    okay(render_template(UnbanTemplate {
        current_user: Some(user),
        ban,
        banned_user,
        banned_by_user,
        error: None,
    }))
}

async fn unban_user_submit(
    s: Session,
    Path(username): Path<String>,
    user_bans: UserBanRepository,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let banned_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    // First fetch the ban and user data for re-rendering on error
    let ban = match user_bans.find_by_user_id(banned_user.id).await {
        Ok(Some(b)) => b,
        Ok(None) => {
            return render_generic_error(s, crate::err::not_found("User is not banned")).await;
        }
        Err(e) => return render_generic_error(s, e).await,
    };

    let banned_by_user = match users.find_by_id(ban.banned_by).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    match user_bans.delete(&user, banned_user.id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            axum::response::Redirect::to("/admin/bans").into_response(),
        ),
        Err(e) => okay(render_template(UnbanTemplate {
            current_user: Some(user.clone()),
            ban,
            banned_user: banned_user.clone(),
            banned_by_user,
            error: Some(e),
        })),
    }
}
