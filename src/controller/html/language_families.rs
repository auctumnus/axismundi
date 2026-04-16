use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, extract::Multipart, headers::UserAgent};
use futures::TryFutureExt;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    embed::{self, GenericEmbed, render_embed, truncate_description},
    err::{AppError, bad_request},
    get_user,
    model::{
        language_families::{
            CreateLanguageFamily, FamilyWithContributors, LanguageFamily, LanguageFamilyRepository,
            SearchLanguageFamilies,
        },
        language_family_invites::{LanguageFamilyInvite, LanguageFamilyInviteRepository},
        language_family_members::LanguageFamilyMemberRepository,
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{
        AppState, BackQuery, ListHeaderKind,
        extract_session::Session,
        s3::S3,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-language-family", post(new_language_family_submit))
        .route(
            "/language-families/{code}/edit",
            post(edit_language_family_submit),
        )
        .route(
            "/language-families/{code}/delete",
            post(delete_language_family_submit),
        )
        .route(
            "/language-families/{code}/change-banner",
            post(change_language_family_banner),
        )
        .route(
            "/language-families/{code}/clear-banner",
            post(clear_language_family_banner),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/language-families", get(search_language_families))
        .route("/new-language-family", get(new_language_family_form))
        .route("/language-families/{code}", get(view_language_family))
        .route(
            "/language-families/{code}/edit",
            get(edit_language_family_form),
        )
        .route(
            "/language-families/{code}/delete",
            get(delete_language_family_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "language_families/fragments/card.html")]
pub struct PreviewCard<'a> {
    pub family_with_contributors: FamilyWithContributors,
    pub back_url: &'a str,
}

#[derive(Template)]
#[template(path = "language_families/fragments/list_header.html")]
#[allow(dead_code)]
pub struct Header<'a> {
    pub current_user: Option<&'a User>,
    pub kind: ListHeaderKind,
}

#[derive(Template)]
#[template(path = "language_families/fragments/breadcrumb.html")]
pub struct Breadcrumb<'a> {
    pub family: &'a LanguageFamily,
}

#[derive(Template)]
#[template(path = "language_families/fragments/footer.html")]
pub struct Footer<'a> {
    pub family: &'a LanguageFamily,
    pub can_edit_family: bool,
}

async fn search_language_families(
    s: Session,
    language_families: LanguageFamilyRepository,
    Query(query): Query<SearchLanguageFamilies>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    let back_url = crate::util::back_url("/language-families", &pagination, &query);

    let results = language_families
        .search(query.clone(), pagination.clone())
        .and_then(|response| {
            response.try_map_async(|family| language_families.materialize(family, s.user()))
        })
        .await;

    let render_item = |family_with_contributors: &FamilyWithContributors| PreviewCard {
        family_with_contributors: family_with_contributors.clone(),
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
        search_name: "families",
        search_action: "/language-families",
        render_item,
    });

    let status = template.status();

    (status, render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/view.html")]
struct ViewLanguageFamilyTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    owner: User,
    contributor_count: usize,
    member_count: usize,
    rendered_description: String,
    can_edit_language_family: bool,
    can_delete_language_family: bool,
    is_liked: bool,
    pending_invite: Option<(LanguageFamilyInvite, User)>,
    family_tree_svg: String,
    json_ld: String,
    back: String,
}

#[allow(clippy::too_many_arguments)]
async fn view_language_family(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    invites: LanguageFamilyInviteRepository,
    members: LanguageFamilyMemberRepository,
    users: UserRepository,
    Path(code): Path<String>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let owner = attempt!(s, users.find_by_id(family.created_by).await);
    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        return okay(
            render_embed(
                embed::EmbedTarget::Discord,
                GenericEmbed {
                    title: family.name,
                    description: format!(
                        "{}\n\n⭐️ {}",
                        truncate_description(&family.description),
                        family.like_count
                    ),
                    author: Some(owner.clone()),
                    color: if owner.gender.is_empty() {
                        None
                    } else {
                        Some(owner.gender.clone())
                    },
                    url: format!(
                        "{}/language-families/{}",
                        &crate::CONFIG.public_url_base,
                        family.code
                    ),
                    image: None,
                },
            )
            .await
            .into_response(),
        );
    }
    let rendered_description = crate::md::render_md(&family.description).unwrap_or_default();

    // Count contributors (non-owner permissions)
    let contributor_count =
        usize::try_from(attempt!(s, permissions.count_contributors(family.id).await)).unwrap_or(0);

    let can_edit_language_family = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let can_delete_language_family = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let is_liked = if let Some(user) = s.user() {
        language_families
            .is_liked(&family.id, &user.id)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    // Check for pending invites
    let pending_invite = if let Some(user) = s.user() {
        match invites
            .find_by_family_and_recipient_unchecked(family.id, user.id)
            .await
        {
            Ok(Some(invite)) if invite.accepted_at.is_none() => {
                match users.find_by_id(invite.sender).await {
                    Ok(sender) => Some((invite, sender)),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let member_count =
        usize::try_from(attempt!(s, members.count_by_family(family.id).await)).unwrap_or(0);

    // Generate family tree SVG
    let family_tree_svg = crate::util::graph_svg::render_family_tree(&family, &members)
        .await
        .unwrap_or_default();

    let json_ld = attempt!(
        s,
        serde_json::to_string(&attempt!(s, language_families.as_json_ld(&family).await))
            .map_err(Into::into)
    );

    let template = ViewLanguageFamilyTemplate {
        current_user: s.user().cloned(),
        family,
        owner,
        contributor_count,
        rendered_description,
        can_edit_language_family,
        can_delete_language_family,
        is_liked,
        pending_invite,
        family_tree_svg,
        member_count,
        json_ld,
        back: back_query.back.unwrap_or_default(),
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/new.html")]
struct NewLanguageFamilyTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
}

async fn new_language_family_form(s: Session) -> (StatusCode, Response) {
    let user = get_user!(s);

    let template = NewLanguageFamilyTemplate {
        current_user: Some(user),
        error: None,
        previous_code: String::new(),
        previous_name: String::new(),
        previous_description: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewLanguageFamilyFormData {
    code: String,
    name: String,
    description: String,
}

async fn new_language_family_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    Form(form): Form<NewLanguageFamilyFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    match language_families
        .create(
            user.clone(),
            CreateLanguageFamily {
                code: form.code.clone(),
                name: form.name.clone(),
                description: form.description.clone(),
            },
        )
        .await
    {
        Ok(family) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let template = NewLanguageFamilyTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_code: form.code,
                previous_name: form.name,
                previous_description: form.description,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/edit.html")]
struct EditLanguageFamilyTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
    can_edit_family: bool,
    will_create_audit_log: bool,
}

async fn edit_language_family_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_family = is_admin_or_mod
        || permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let template = EditLanguageFamilyTemplate {
        current_user: Some(user),
        family: family.clone(),
        error: None,
        previous_code: family.code,
        previous_name: family.name,
        previous_description: family.description,
        can_edit_family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditLanguageFamilyFormData {
    code: String,
    name: String,
    description: String,
}

async fn edit_language_family_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
    Form(form): Form<EditLanguageFamilyFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_family = is_admin_or_mod
        || permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    match language_families
        .update(
            &user,
            family.id,
            crate::model::language_families::UpdateLanguageFamily {
                code: if form.code == family.code {
                    None
                } else {
                    Some(form.code.clone())
                },
                name: if form.name == family.name {
                    None
                } else {
                    Some(form.name.clone())
                },
                description: if form.description == family.description {
                    None
                } else {
                    Some(form.description.clone())
                },
            },
        )
        .await
    {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", updated.code)).into_response(),
        ),
        Err(e) => {
            let template = EditLanguageFamilyTemplate {
                current_user: Some(user),
                family,
                error: Some(e),
                previous_code: form.code,
                previous_name: form.name,
                previous_description: form.description,
                can_edit_family,
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/delete.html")]
struct DeleteLanguageFamilyTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    can_delete_family: bool,
    will_create_audit_log: bool,
}

async fn delete_language_family_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_delete_family = is_admin_or_mod
        || permissions
            .has_permission(user.id, family.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let template = DeleteLanguageFamilyTemplate {
        current_user: Some(user),
        family,
        can_delete_family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_language_family_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match language_families.delete(&user, family.id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/language-families").into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}

const MAX_BANNER_SIZE: usize = 5 * 1024 * 1024;

async fn change_language_family_banner(
    s: Session,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
    mut multipart: Multipart,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await.ok().flatten() {
        if field.name().unwrap_or("") == "banner" {
            content_type = field.content_type().map(std::string::ToString::to_string);
            match field.bytes().await {
                Ok(bytes) => file_data = Some(bytes.to_vec()),
                Err(e) => {
                    return crate::controller::html::render_generic_error(
                        s,
                        bad_request(format!("Failed to read file: {e}")),
                    )
                    .await;
                }
            }
            break;
        }
    }

    let Some(file_data) = file_data else {
        return crate::controller::html::render_generic_error(
            s,
            bad_request("No banner file provided"),
        )
        .await;
    };
    let Some(content_type) = content_type else {
        return crate::controller::html::render_generic_error(
            s,
            bad_request("No content type provided"),
        )
        .await;
    };

    if file_data.len() > MAX_BANNER_SIZE {
        return crate::controller::html::render_generic_error(
            s,
            bad_request("File size exceeds the maximum limit of 5MB"),
        )
        .await;
    }

    let filename = match S3
        .upload_banner("family", family.id, &file_data, &content_type)
        .await
    {
        Ok(f) => f,
        Err(e) => return crate::controller::html::render_generic_error(s, e).await,
    };

    match language_families
        .update_banner(&user, family.id, &filename)
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{code}/edit")).into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}

async fn clear_language_family_banner(
    s: Session,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match language_families
        .update_banner(&user, family.id, "")
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{code}/edit")).into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}
