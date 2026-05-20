use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures::TryFutureExt;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    err::{AppError, forbidden},
    get_user,
    model::{
        news::{CreateNews, News, NewsRepository, NewsSearch, NewsWithCreator, UpdateNews},
        users::User,
    },
    pagination::PaginatedRequest,
    util::{
        AppState, BackQuery, ListHeaderKind,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-news", post(new_news_submit))
        .route("/news/{slug}/edit", post(edit_news_submit))
        .route("/news/{slug}/delete", post(delete_news_submit))
        .route("/news/{slug}/publish", post(publish_submit))
        .route("/news/{slug}/unpublish", post(unpublish_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/new-news", get(new_news_form))
        .route("/news", get(search_news))
        .route("/news/{slug}", get(view_news))
        .route("/news/{slug}/edit", get(edit_news_form))
        .route("/news/{slug}/delete", get(delete_news_form))
        .route("/news/{slug}/publish", get(publish_form))
        .route("/news/{slug}/unpublish", get(unpublish_form));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "news/fragments/card.html")]
pub struct PreviewCard<'a> {
    pub news_with_creator: NewsWithCreator,
    pub back_url: &'a str,
}

#[derive(Template)]
#[template(path = "news/fragments/list_header.html")]
#[allow(dead_code)]
pub struct Header<'a> {
    pub current_user: Option<&'a User>,
    pub kind: ListHeaderKind,
}

#[derive(Template)]
#[template(path = "news/new.html")]
#[allow(dead_code)]
struct NewNewsTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_title: String,
    previous_content: String,
    previous_as_draft: bool,
}

async fn new_news_form(s: Session) -> (StatusCode, Response) {
    let Some(user) = s.user().cloned() else {
        return (
            StatusCode::UNAUTHORIZED,
            Redirect::to("/login").into_response(),
        );
    };

    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(s, forbidden("only admins or moderators can create news"))
            .await;
    }

    let template = NewNewsTemplate {
        current_user: Some(user),
        error: None,
        previous_title: String::new(),
        previous_content: String::new(),
        previous_as_draft: false,
    };
    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewNewsFormData {
    title: String,
    content: String,
    as_draft: Option<String>,
}

async fn new_news_submit(
    s: Session,
    news: NewsRepository,
    Form(form): Form<NewNewsFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let as_draft = form.as_draft.is_some();

    let payload = CreateNews {
        title: form.title.clone(),
        content: form.content.clone(),
        as_draft,
    };

    match news.create(&user, payload).await {
        Ok(article) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/news/{}", article.slug)).into_response(),
        ),
        Err(e) => {
            let status_code = e.status_code;
            let template = NewNewsTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_title: form.title,
                previous_content: form.content,
                previous_as_draft: as_draft,
            };
            (status_code, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "news/view.html")]
#[allow(dead_code)]
struct ViewNewsTemplate {
    current_user: Option<User>,
    news_with_creator: NewsWithCreator,
    rendered_content: String,
    is_admin_or_mod: bool,
    back: String,
}

async fn view_news(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let article = attempt!(s, news.find_by_slug_for(&slug, s.user()).await);
    let rendered_content = crate::md::render_md(&article.content).unwrap_or_default();
    let news_with_creator = attempt!(s, news.materialize(article).await);

    let is_admin_or_mod = s
        .user()
        .is_some_and(|u| u.is_admin() || u.is_moderator());

    let template = ViewNewsTemplate {
        current_user: s.user().cloned(),
        news_with_creator,
        rendered_content,
        is_admin_or_mod,
        back: back_query.back.unwrap_or_default(),
    };
    okay(render_template(template))
}

async fn search_news(
    s: Session,
    news: NewsRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<NewsSearch>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let back_url = crate::util::back_url("/news", &pagination, &query);

    let results = news
        .search(pagination.clone(), query.clone(), s.user())
        .and_then(|response| response.try_map_async(|article| news.materialize(article)))
        .await;

    let render_item = |news_with_creator: &NewsWithCreator| PreviewCard {
        news_with_creator: news_with_creator.clone(),
        back_url: &back_url,
    };

    let header = Header {
        current_user: current_user.as_ref(),
        kind: ListHeaderKind::Search,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user: current_user.clone(),
        header,
        query_template: query.clone(),
        query,
        results,
        pagination,
        search_name: "news",
        search_action: "/news",
        render_item,
    });

    let status = template.status();
    (status, render_template(template))
}

#[derive(Template)]
#[template(path = "news/edit.html")]
#[allow(dead_code)]
struct EditNewsTemplate {
    current_user: Option<User>,
    news: News,
    error: Option<AppError>,
    previous_title: String,
    previous_content: String,
}

async fn edit_news_form(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(s, forbidden("only admins or moderators can edit news"))
            .await;
    }
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);

    let template = EditNewsTemplate {
        current_user: Some(user),
        previous_title: article.title.clone(),
        previous_content: article.content.clone(),
        news: article,
        error: None,
    };
    okay(render_template(template))
}

async fn edit_news_submit(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
    Form(updates): Form<UpdateNews>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);

    match news.update(&user, article.id, updates.clone()).await {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/news/{}", result.slug)).into_response(),
        ),
        Err(e) => {
            let status = e.status_code;
            let template = EditNewsTemplate {
                current_user: Some(user),
                previous_title: updates.title.unwrap_or(article.title.clone()),
                previous_content: updates.content.unwrap_or(article.content.clone()),
                news: article,
                error: Some(e),
            };
            (status, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "news/publish.html")]
#[allow(dead_code)]
struct PublishNewsTemplate {
    current_user: Option<User>,
    news: News,
}

#[derive(Template)]
#[template(path = "news/unpublish.html")]
#[allow(dead_code)]
struct UnpublishNewsTemplate {
    current_user: Option<User>,
    news: News,
}

async fn publish_form(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(s, forbidden("only admins or moderators can publish news"))
            .await;
    }
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);
    let template = PublishNewsTemplate {
        current_user: Some(user),
        news: article,
    };
    okay(render_template(template))
}

async fn publish_submit(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);
    attempt!(s, news.publish(&user, article.id).await);
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/news/{}", article.slug)).into_response(),
    )
}

async fn unpublish_form(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(s, forbidden("only admins or moderators can unpublish news"))
            .await;
    }
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);
    let template = UnpublishNewsTemplate {
        current_user: Some(user),
        news: article,
    };
    okay(render_template(template))
}

async fn unpublish_submit(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);
    attempt!(s, news.unpublish(&user, article.id).await);
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/news/{}", article.slug)).into_response(),
    )
}

#[derive(Template)]
#[template(path = "news/delete.html")]
#[allow(dead_code)]
struct DeleteNewsTemplate {
    current_user: Option<User>,
    news: News,
}

async fn delete_news_form(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !(user.is_admin() || user.is_moderator()) {
        return render_generic_error(s, forbidden("only admins or moderators can delete news"))
            .await;
    }
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);
    let template = DeleteNewsTemplate {
        current_user: Some(user),
        news: article,
    };
    okay(render_template(template))
}

async fn delete_news_submit(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let article = attempt!(s, news.find_by_slug_for(&slug, Some(&user)).await);

    match news.delete(&user, article).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/news").into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
