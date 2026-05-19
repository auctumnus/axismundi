use askama::Template;
use axum::{
    Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::AppState;
use crate::controller::html::{okay, render_generic_error, render_template};
use crate::err::{AppError, forbidden};
use crate::get_user;
use crate::model::user_tags::{CreateUserTag, UserTag, UserTagRepository};
use crate::model::users::{User, UserRepository};
use crate::util::extract_session::Session;
use axum::Form;

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/admin/users/{username}/tags", post(add_tag_submit))
        .route(
            "/admin/users/{username}/tags/{tag}/delete",
            post(delete_tag_submit),
        )
        .route(
            "/admin/users/{username}/make-moderator",
            post(make_moderator_submit),
        );

    let normal_routes = Router::new()
        .route("/admin/users/{username}/tags", get(edit_tags_form))
        .route(
            "/admin/users/{username}/tags/{tag}/delete",
            get(delete_tag_form),
        )
        .route(
            "/admin/users/{username}/make-moderator",
            get(make_moderator_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "user_tags/edit.html")]
struct EditTagsTemplate {
    current_user: Option<User>,
    target_user: User,
    tags: Vec<UserTag>,
    error: Option<AppError>,
    previous_tag: Option<String>,
    previous_hidden: bool,
}

async fn render_edit_page(
    s: Session,
    user: User,
    user_tags: &UserTagRepository,
    users: &UserRepository,
    username: &str,
    error: Option<AppError>,
    previous_tag: Option<String>,
    previous_hidden: bool,
) -> (StatusCode, Response) {
    let target_user = match users.find_by_username(username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    let tags = match user_tags.find_all_by_user_id(target_user.id).await {
        Ok(t) => t,
        Err(e) => return render_generic_error(s, e).await,
    };

    okay(render_template(EditTagsTemplate {
        current_user: Some(user),
        target_user,
        tags,
        error,
        previous_tag,
        previous_hidden,
    }))
}

async fn edit_tags_form(
    s: Session,
    Path(username): Path<String>,
    user_tags: UserTagRepository,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(&s);

    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(
            s,
            forbidden("You do not have permission to edit user tags."),
        )
        .await;
    }

    render_edit_page(s, user, &user_tags, &users, &username, None, None, false).await
}

#[derive(Deserialize)]
struct AddTagForm {
    tag: String,
    #[serde(default)]
    hidden: Option<String>,
}

async fn add_tag_submit(
    s: Session,
    Path(username): Path<String>,
    user_tags: UserTagRepository,
    users: UserRepository,
    Form(form): Form<AddTagForm>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let target_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    let tag = form.tag.trim().to_string();
    let hidden = form.hidden.is_some();

    let req = CreateUserTag {
        tag: tag.clone(),
        hidden,
    };

    match user_tags.create(&user, target_user.id, req).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/admin/users/{username}/tags")).into_response(),
        ),
        Err(e) => {
            render_edit_page(
                s,
                user,
                &user_tags,
                &users,
                &username,
                Some(e),
                Some(tag),
                hidden,
            )
            .await
        }
    }
}

#[derive(Template)]
#[template(path = "user_tags/delete.html")]
struct DeleteTagTemplate {
    current_user: Option<User>,
    target_user: User,
    tag: String,
    error: Option<AppError>,
}

async fn delete_tag_form(
    s: Session,
    Path((username, tag)): Path<(String, String)>,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(&s);

    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(
            s,
            forbidden("You do not have permission to edit user tags."),
        )
        .await;
    }

    let target_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    okay(render_template(DeleteTagTemplate {
        current_user: Some(user),
        target_user,
        tag,
        error: None,
    }))
}

async fn delete_tag_submit(
    s: Session,
    Path((username, tag)): Path<(String, String)>,
    user_tags: UserTagRepository,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let target_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    match user_tags.delete(&user, &target_user, tag.clone()).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/admin/users/{username}/tags")).into_response(),
        ),
        Err(e) => okay(render_template(DeleteTagTemplate {
            current_user: Some(user),
            target_user,
            tag,
            error: Some(e),
        })),
    }
}

#[derive(Template)]
#[template(path = "user_tags/make_moderator.html")]
struct MakeModeratorTemplate {
    current_user: Option<User>,
    target_user: User,
    error: Option<AppError>,
}

async fn make_moderator_form(
    s: Session,
    Path(username): Path<String>,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(&s);

    if !user.is_admin() {
        return render_generic_error(s, forbidden("Only admins can grant the moderator tag."))
            .await;
    }

    let target_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    okay(render_template(MakeModeratorTemplate {
        current_user: Some(user),
        target_user,
        error: None,
    }))
}

async fn make_moderator_submit(
    s: Session,
    Path(username): Path<String>,
    user_tags: UserTagRepository,
    users: UserRepository,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let target_user = match users.find_by_username(&username).await {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    let req = CreateUserTag {
        tag: "moderator".to_string(),
        hidden: false,
    };

    match user_tags.create(&user, target_user.id, req).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/users/{username}")).into_response(),
        ),
        Err(e) => okay(render_template(MakeModeratorTemplate {
            current_user: Some(user),
            target_user,
            error: Some(e),
        })),
    }
}
