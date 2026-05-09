use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use futures::TryFutureExt;

use crate::{
    attempt,
    controller::html::{self, LanguagesWithContributors, okay, render_template},
    err::AppError,
    get_user,
    model::{
        contribution_stats::ContributionStatsRepository,
        language_families::{FamilyWithContributors, LanguageFamily, LanguageFamilyRepository},
        language_family_members::{
            LanguageFamilyMemberRepository, LanguageFamilyRelationType, MemberWithLanguages,
            SearchLanguageFamilyMembers,
        },
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
    },
    pagination::PaginatedRequest,
    util::{
        AppState, ListHeaderKind,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

pub fn create_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route("/languages/{code}/relatives", get(search_relatives))
        .route(
            "/language-families/{code}/add-root/language",
            get(add_root_language_form),
        )
        .route(
            "/language-families/{code}/add-root/language",
            post(add_root_language_submit),
        )
        .route(
            "/language-families/{code}/add-root/grouping",
            get(add_root_grouping_form),
        )
        .route(
            "/language-families/{code}/add-root/grouping",
            post(add_root_grouping_submit),
        )
        .route("/language-families/{code}/members/{id}", get(view_member))
        .route(
            "/language-families/{code}/members/{id}/add-language-member",
            post(add_language_member_submit),
        )
        .route(
            "/language-families/{code}/members/{id}/add-grouping",
            post(add_grouping_submit),
        )
        .route(
            "/language-families/{code}/members/{id}/delete",
            post(delete_member_submit),
        )
        .route("/language-families/{code}/members", get(search_members))
        .route(
            "/language-families/{code}/members/new",
            get(new_member_form),
        )
        .route(
            "/language-families/{code}/members/new",
            post(new_member_submit),
        )
        .route("/language-families/{code}/add-root", get(add_root_form))
        .route(
            "/language-families/{code}/members/{id}/add-child",
            get(add_child_form),
        )
        .route(
            "/language-families/{code}/members/{id}/add-language-member",
            get(add_language_member_form),
        )
        .route(
            "/language-families/{code}/members/{id}/add-grouping",
            get(add_grouping_form),
        )
        .route(
            "/language-families/{code}/members/{id}/delete",
            get(delete_member_form),
        )
        .route(
            "/language-families/{code}/members/{id}/convert-to-grouping",
            get(convert_to_grouping_form),
        )
        .route(
            "/language-families/{code}/members/{id}/convert-to-grouping",
            post(convert_to_grouping_submit),
        )
        .route(
            "/language-families/{code}/members/{id}/convert-to-language",
            get(convert_to_language_form),
        )
        .route(
            "/language-families/{code}/members/{id}/convert-to-language",
            post(convert_to_language_submit),
        )
        .route(
            "/language-families/{code}/members/{id}/swap-with-parent",
            get(swap_with_parent_form),
        )
        .route(
            "/language-families/{code}/members/{id}/swap-with-parent",
            post(swap_with_parent_submit),
        )
        .route(
            "/language-families/{code}/members/{id}/change-parent",
            get(change_parent_form),
        )
        .route(
            "/language-families/{code}/members/{id}/change-parent",
            post(change_parent_submit),
        )
        .route(
            "/language-families/{code}/members/{id}/edit",
            get(edit_member_form),
        )
        .route(
            "/language-families/{code}/members/{id}/edit",
            post(edit_member_submit),
        )
}

#[derive(Template)]
#[template(path = "language_families/members/view.html")]
struct ViewMemberTemplate {
    name: String,
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    can_edit_family: bool,
    can_edit_language: bool,
    language: Option<LanguagesWithContributors>,
    parent_member: Option<MemberWithLanguages>,
    children: Vec<MemberWithLanguages>,
    member_scs: Option<crate::model::sound_change_sets::SoundChangeSet>,
}

#[allow(clippy::too_many_arguments)]
async fn view_member(
    State(state): State<AppState>,
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    contribution_stats: ContributionStatsRepository,
    languages: LanguageRepository,
    permissions: LanguageFamilyPermissionRepository,
    sets: crate::model::sound_change_sets::SoundChangeSetRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member = attempt!(s, members.find_by_id(id).await);
    let member = attempt!(s, members.materialize(member).await);

    let can_edit_family = if let Some(user) = s.user() {
        attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Editor)
                .await
        )
    } else {
        false
    };

    let language = if let Some(language) = &member.language {
        let top_contributors = attempt!(
            s,
            contribution_stats
                .get_top_contributors(&language.id, 5)
                .await
        );
        let is_liked = if let Some(user) = s.user() {
            attempt!(s, languages.is_liked(&language.id, &user.id).await)
        } else {
            false
        };
        let language_with_contributors = LanguagesWithContributors {
            language: language.clone(),
            top_contributors,
            is_liked,
        };

        Some(language_with_contributors)
    } else {
        None
    };

    let can_edit_language = if let (Some(user), Some(language)) = (s.user(), &member.language) {
        LanguagePermissionRepository::new(state.clone())
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let parent_member = if let Some(parent_id) = member.member.parent_member_id() {
        let parent_raw = attempt!(s, members.find_by_id(parent_id).await);
        Some(attempt!(s, members.materialize(parent_raw).await))
    } else {
        None
    };

    let name = member.name();
    let get_five = PaginatedRequest {
        limit: 5,
        offset: 0,
    };

    let children = attempt!(
        s,
        members
            .search(
                SearchLanguageFamilyMembers {
                    family_code: Some(code.clone()),
                    q: None,
                    parent_language_code: None,
                    parent_member_id: Some(member.member.id()),
                    language_code: None,
                    relation_type: None,
                },
                get_five,
            )
            .await
    );
    let mut materialized_children = Vec::new();
    for child in children.items {
        if let Ok(m) = members.materialize(child).await {
            materialized_children.push(m);
        }
    }

    let family = attempt!(s, language_families.materialize(family, s.user()).await);

    let member_scs = sets.get_for_member(id).await.ok().flatten();

    let template = ViewMemberTemplate {
        name,
        current_user: s.user().cloned(),
        family,
        member,
        can_edit_family,
        language,
        can_edit_language,
        parent_member,
        children: materialized_children,
        member_scs,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/members/fragments/card.html")]
struct MemberPreviewCard<'a> {
    member_with_languages: MemberWithLanguages,
    back_url: &'a str,
}

#[derive(Template)]
#[template(path = "language_families/members/fragments/list_header.html")]
struct MemberHeader<'a> {
    can_edit_family: bool,
    family: &'a LanguageFamily,
    kind: ListHeaderKind,
}

impl MemberHeader<'_> {
    fn title(&self) -> &'static str {
        match self.kind {
            ListHeaderKind::Preview => "members",
            ListHeaderKind::Search => "search members",
        }
    }
}

async fn search_members(
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
    Query(query): Query<SearchLanguageFamilyMembers>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit_family = if let Some(user) = &current_user {
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let search_action = format!("/language-families/{}/members", code);
    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let search_query = SearchLanguageFamilyMembers {
        family_code: Some(code.clone()),
        ..query.clone()
    };

    let results = members
        .search(search_query, pagination.clone())
        .and_then(|response| response.try_map_async(|member| members.materialize(member)))
        .await;

    let render_item = |member_with_languages: &MemberWithLanguages| MemberPreviewCard {
        member_with_languages: member_with_languages.clone(),
        back_url: &back_url,
    };

    let header = MemberHeader {
        can_edit_family,
        family: &family,
        kind: ListHeaderKind::Search,
    };

    let breadcrumbs = html::language_families::Breadcrumb { family: &family };
    let footer = html::language_families::Footer {
        family: &family,
        can_edit_family,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template: query.clone(),
        query,
        results,
        pagination,
        search_name: "members",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();

    (status, render_template(template))
}

#[derive(Template)]
#[template(path = "languages/fragments/relatives_header.html")]
struct RelativesHeader<'a> {
    language: &'a LanguagesWithContributors,
}

async fn search_relatives(
    s: Session,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    Path(code): Path<String>,
    Query(query): Query<SearchLanguageFamilyMembers>,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&code).await);
    let language = attempt!(s, languages.materialize(language, s.user()).await);

    let search_action = format!("/languages/{}/relatives", code);
    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let search_query = SearchLanguageFamilyMembers {
        language_code: Some(code.clone()),
        ..query.clone()
    };

    let results = members
        .search(search_query, pagination.clone())
        .and_then(|response| response.try_map_async(|member| members.materialize(member)))
        .await;

    let render_item = |member_with_languages: &MemberWithLanguages| MemberPreviewCard {
        member_with_languages: member_with_languages.clone(),
        back_url: &back_url,
    };

    let header = RelativesHeader {
        language: &language,
    };

    let breadcrumbs = html::languages::Breadcrumb {
        language: &language.language,
    };
    let footer = html::languages::Footer {
        language: &language.language,
        can_edit_language: false,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template: query.clone(),
        query,
        results,
        pagination,
        search_name: "relatives",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();

    (status, render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/members/add-root.html")]
struct AddRootTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    will_create_audit_log: bool,
}

async fn add_root_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddRootTemplate {
        current_user: Some(user),
        family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/members/add-root-language.html")]
struct AddRootLanguageTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    error: Option<AppError>,
    will_create_audit_log: bool,
    available_languages: Vec<Language>,
    previous_language_code: String,
    previous_notes: String,
}

async fn add_root_language_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddRootLanguageTemplate {
        current_user: Some(user),
        family,
        error: None,
        will_create_audit_log,
        available_languages,
        previous_language_code: String::new(),
        previous_notes: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddRootLanguageFormData {
    language_code: String,
    notes: Option<String>,
}

async fn add_root_language_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    Path(code): Path<String>,
    Form(form): Form<AddRootLanguageFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members
        .create(
            user.clone(),
            family.clone(),
            None,
            crate::model::language_family_members::CreateLanguageFamilyMember {
                language_code: Some(form.language_code.clone()),
                title: None,
                relation_type: LanguageFamilyRelationType::Descendant,
                notes: form.notes.clone().filter(|s| !s.is_empty()),
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let available_languages = languages
                .list_editable_by_user(user.id)
                .await
                .unwrap_or_default();
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddRootLanguageTemplate {
                current_user: Some(user),
                family,
                error: Some(e),
                will_create_audit_log,
                available_languages,
                previous_language_code: form.language_code,
                previous_notes: form.notes.unwrap_or_default(),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/add-root-grouping.html")]
struct AddRootGroupingTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    error: Option<AppError>,
    will_create_audit_log: bool,
    previous_title: String,
    previous_notes: String,
}

async fn add_root_grouping_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddRootGroupingTemplate {
        current_user: Some(user),
        family,
        error: None,
        will_create_audit_log,
        previous_title: String::new(),
        previous_notes: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddRootGroupingFormData {
    title: String,
    notes: Option<String>,
}

async fn add_root_grouping_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path(code): Path<String>,
    Form(form): Form<AddRootGroupingFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members
        .create(
            user.clone(),
            family.clone(),
            None,
            crate::model::language_family_members::CreateLanguageFamilyMember {
                language_code: None,
                title: Some(form.title.clone()),
                relation_type: LanguageFamilyRelationType::Descendant,
                notes: form.notes.clone().filter(|s| !s.is_empty()),
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddRootGroupingTemplate {
                current_user: Some(user),
                family,
                error: Some(e),
                will_create_audit_log,
                previous_title: form.title,
                previous_notes: form.notes.unwrap_or_default(),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/add.html")]
#[allow(dead_code)]
struct AddChildTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    can_edit_family: bool,
}

async fn add_child_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddChildTemplate {
        current_user: Some(user),
        family,
        parent_member,
        error: None,
        will_create_audit_log,
        can_edit_family,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/members/add-language-member.html")]
#[allow(dead_code)]
struct AddLanguageMemberTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    available_languages: Vec<Language>,
    previous_language_code: String,
    previous_is_hybrid: Option<bool>,
    previous_notes: String,
    can_edit_family: bool,
}

async fn add_language_member_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddLanguageMemberTemplate {
        current_user: Some(user),
        family,
        parent_member,
        error: None,
        will_create_audit_log,
        available_languages,
        previous_language_code: String::new(),
        previous_is_hybrid: None,
        previous_notes: String::new(),
        can_edit_family,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddLanguageMemberFormData {
    language_code: String,
    #[serde(default)]
    is_hybrid: Option<String>,
    notes: String,
}

#[allow(clippy::too_many_arguments)]
async fn add_language_member_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<AddLanguageMemberFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let is_hybrid = form.is_hybrid.as_ref().is_some_and(|v| v == "true");
    let relation_type = if is_hybrid {
        LanguageFamilyRelationType::Hybrid
    } else {
        LanguageFamilyRelationType::Descendant
    };

    match members
        .create(
            user.clone(),
            family.clone(),
            Some(member_id),
            crate::model::language_family_members::CreateLanguageFamilyMember {
                language_code: Some(form.language_code.clone()),
                title: None,
                relation_type,
                notes: if form.notes.is_empty() {
                    None
                } else {
                    Some(form.notes.clone())
                },
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let can_edit_family = permissions
                .has_permission(user.id, family.id, PermissionLevel::Editor)
                .await
                .unwrap_or(false);
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let available_languages = languages
                .list_editable_by_user(user.id)
                .await
                .unwrap_or_default();
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddLanguageMemberTemplate {
                current_user: Some(user),
                family,
                parent_member,
                error: Some(e),
                will_create_audit_log,
                available_languages,
                previous_language_code: form.language_code,
                previous_is_hybrid: Some(is_hybrid),
                previous_notes: form.notes,
                can_edit_family,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/add-grouping.html")]
#[allow(dead_code)]
struct AddGroupingTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    previous_title: String,
    previous_notes: String,
    can_edit_family: bool,
}

async fn add_grouping_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddGroupingTemplate {
        current_user: Some(user),
        family,
        parent_member,
        error: None,
        will_create_audit_log,
        previous_title: String::new(),
        previous_notes: String::new(),
        can_edit_family: true,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddGroupingFormData {
    title: String,
    notes: Option<String>,
}

async fn add_grouping_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<AddGroupingFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    match members
        .create(
            user.clone(),
            family.clone(),
            Some(member_id),
            crate::model::language_family_members::CreateLanguageFamilyMember {
                language_code: None,
                title: Some(form.title.clone()),
                relation_type: LanguageFamilyRelationType::Descendant,
                notes: form.notes.clone().filter(|s| !s.is_empty()),
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddGroupingTemplate {
                current_user: Some(user),
                family,
                parent_member,
                error: Some(e),
                will_create_audit_log,
                previous_title: form.title,
                previous_notes: form.notes.unwrap_or_default(),
                can_edit_family: true,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/delete.html")]
struct DeleteMemberTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    can_edit_family: bool,
    will_create_audit_log: bool,
}

async fn delete_member_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to delete members from this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = DeleteMemberTemplate {
        current_user: Some(user),
        family,
        member,
        can_edit_family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_member_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members.delete(&user, member_id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}

#[derive(Template)]
#[template(path = "language_families/members/convert-to-grouping.html")]
#[allow(dead_code)]
struct ConvertToGroupingTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
}

async fn convert_to_grouping_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    if member.member.as_language().is_none() {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::bad_request("this member is already a grouping"),
        )
        .await;
    }

    if member.language.is_none() {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::internal_error("language node is missing its language"),
        )
        .await;
    }

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to edit members in this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = ConvertToGroupingTemplate {
        current_user: Some(user),
        family,
        member,
        error: None,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn convert_to_grouping_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    match members
        .convert_to_grouping(&user, member_id, member.name(), member.notes.clone())
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/language-families/{}/members/{}",
                family.code, member_id
            ))
            .into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = ConvertToGroupingTemplate {
                current_user: Some(user),
                family,
                member,
                error: Some(e),
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/convert-to-language.html")]
#[allow(dead_code)]
struct ConvertToLanguageTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    available_languages: Vec<Language>,
    previous_language_code: String,
}

async fn convert_to_language_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    if member.member.as_grouping().is_none() {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::bad_request("this member is already a language node"),
        )
        .await;
    }

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to edit members in this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let available_languages = attempt!(
        s,
        languages
            .list_editable_by_user_without_family(user.id)
            .await
    );
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = ConvertToLanguageTemplate {
        current_user: Some(user),
        family,
        member,
        error: None,
        will_create_audit_log,
        available_languages,
        previous_language_code: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct ConvertToLanguageFormData {
    language_code: String,
}

async fn convert_to_language_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<ConvertToLanguageFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    match members
        .convert_to_language(&user, member_id, form.language_code.clone())
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/language-families/{}/members/{}",
                family.code, member_id
            ))
            .into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let available_languages = languages
                .list_editable_by_user_without_family(user.id)
                .await
                .unwrap_or_default();
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = ConvertToLanguageTemplate {
                current_user: Some(user),
                family,
                member,
                error: Some(e),
                will_create_audit_log,
                available_languages,
                previous_language_code: form.language_code,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/swap-with-parent.html")]
#[allow(dead_code)]
struct SwapWithParentTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
}

async fn swap_with_parent_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);

    let parent_id = match member_raw.parent_member_id() {
        Some(id) => id,
        None => {
            return crate::controller::html::render_generic_error(
                s,
                crate::err::bad_request("this member has no parent to swap with"),
            )
            .await;
        }
    };

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("you don't have permission to edit members in this family"),
        )
        .await;
    }

    let member = attempt!(s, members.materialize(member_raw).await);
    let parent_raw = attempt!(s, members.find_by_id(parent_id).await);
    let parent_member = attempt!(s, members.materialize(parent_raw).await);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    okay(render_template(SwapWithParentTemplate {
        current_user: Some(user),
        family,
        member,
        parent_member,
        error: None,
        will_create_audit_log,
    }))
}

async fn swap_with_parent_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members.swap_with_parent(&user, member_id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/language-families/{}/members/{}",
                family.code, member_id
            ))
            .into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}

#[derive(Template)]
#[template(path = "language_families/members/change-parent.html")]
#[allow(dead_code)]
struct ChangeParentTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    available_members: Vec<MemberWithLanguages>,
    error: Option<AppError>,
    previous_parent_id: String,
    will_create_audit_log: bool,
}

#[allow(irrefutable_let_patterns)]
async fn change_parent_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    use std::collections::{HashMap, HashSet};

    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("you don't have permission to edit members in this family"),
        )
        .await;
    }

    // compute descendants of member_id so we can exclude them from the picker
    let tree_schema = attempt!(s, family.tree_schema());
    let adjacency_list: HashMap<Uuid, Vec<Uuid>> = {
        use crate::model::language_families::LanguageFamilyInner;
        if let LanguageFamilyInner::V1(ref v1) = tree_schema {
            let mut map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
            for e in &v1.edges {
                if let Some(pid) = e.parent_member_id {
                    map.entry(pid).or_default().push(e.child_member_id);
                }
            }
            map
        } else {
            HashMap::new()
        }
    };

    let all_raw = attempt!(s, members.all_for_family(family.id).await);

    // collect all descendants (including the member itself) to exclude
    let mut excluded: HashSet<Uuid> = HashSet::new();
    excluded.insert(member_id);
    for candidate in &all_raw {
        let cid = candidate.id();
        if cid != member_id
            && crate::util::dfs(&adjacency_list, member_id, cid, &mut HashMap::new())
        {
            excluded.insert(cid);
        }
    }

    let mut available_members = Vec::new();
    for raw in all_raw {
        if excluded.contains(&raw.id()) {
            continue;
        }
        if let Ok(m) = members.materialize(raw).await {
            available_members.push(m);
        }
    }

    let previous_parent_id = member_raw
        .parent_member_id()
        .map(|id| id.to_string())
        .unwrap_or_default();

    let member = attempt!(s, members.materialize(member_raw).await);
    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    okay(render_template(ChangeParentTemplate {
        current_user: Some(user),
        family,
        member,
        available_members,
        error: None,
        previous_parent_id,
        will_create_audit_log,
    }))
}

#[derive(Deserialize)]
struct ChangeParentFormData {
    new_parent_id: Uuid,
}

#[allow(irrefutable_let_patterns)]
async fn change_parent_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<ChangeParentFormData>,
) -> (StatusCode, Response) {
    use std::collections::{HashMap, HashSet};

    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members
        .change_parent(&user, member_id, form.new_parent_id)
        .await
    {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/language-families/{}/members/{}",
                family.code, member_id
            ))
            .into_response(),
        ),
        Err(e) => {
            let member_raw = attempt!(s, members.find_by_id(member_id).await);
            let tree_schema = attempt!(s, family.tree_schema());
            let adjacency_list: HashMap<Uuid, Vec<Uuid>> = {
                use crate::model::language_families::LanguageFamilyInner;
                if let LanguageFamilyInner::V1(ref v1) = tree_schema {
                    let mut map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
                    for edge in &v1.edges {
                        if let Some(pid) = edge.parent_member_id {
                            map.entry(pid).or_default().push(edge.child_member_id);
                        }
                    }
                    map
                } else {
                    HashMap::new()
                }
            };
            let all_raw = attempt!(s, members.all_for_family(family.id).await);
            let mut excluded: HashSet<Uuid> = HashSet::new();
            excluded.insert(member_id);
            for candidate in &all_raw {
                let cid = candidate.id();
                if cid != member_id
                    && crate::util::dfs(&adjacency_list, member_id, cid, &mut HashMap::new())
                {
                    excluded.insert(cid);
                }
            }
            let mut available_members = Vec::new();
            for raw in all_raw {
                if excluded.contains(&raw.id()) {
                    continue;
                }
                if let Ok(m) = members.materialize(raw).await {
                    available_members.push(m);
                }
            }
            let previous_parent_id = form.new_parent_id.to_string();
            let member = attempt!(s, members.materialize(member_raw).await);
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);
            let template = ChangeParentTemplate {
                current_user: Some(user),
                family,
                member,
                available_members,
                error: Some(e),
                previous_parent_id,
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/edit.html")]
#[allow(dead_code)]
struct EditMemberTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    previous_title: String,
    previous_notes: String,
}

async fn edit_member_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("you don't have permission to edit members in this family"),
        )
        .await;
    }

    let previous_title = match &member.member {
        crate::model::language_family_members::LanguageFamilyMember::Grouping(g) => g.title.clone(),
        crate::model::language_family_members::LanguageFamilyMember::Language(_) => String::new(),
    };
    let previous_notes = member.member.notes().to_string();

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    okay(render_template(EditMemberTemplate {
        current_user: Some(user),
        family,
        member,
        error: None,
        will_create_audit_log,
        previous_title,
        previous_notes,
    }))
}

#[derive(Deserialize)]
struct EditMemberFormData {
    title: Option<String>,
    notes: Option<String>,
}

async fn edit_member_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<EditMemberFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    let notes = form.notes.clone().unwrap_or_default();

    match members
        .update(&user, member_id, form.title.clone(), notes.clone())
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/language-families/{}/members/{}",
                family.code, member_id
            ))
            .into_response(),
        ),
        Err(e) => {
            let previous_title = form.title.clone().unwrap_or_else(|| match &member.member {
                crate::model::language_family_members::LanguageFamilyMember::Grouping(g) => {
                    g.title.clone()
                }
                crate::model::language_family_members::LanguageFamilyMember::Language(_) => {
                    String::new()
                }
            });
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);
            let template = EditMemberTemplate {
                current_user: Some(user),
                family,
                member,
                error: Some(e),
                will_create_audit_log,
                previous_title,
                previous_notes: notes,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/new.html")]
#[allow(dead_code)]
struct NewMemberTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    available_members: Vec<MemberWithLanguages>,
    error: Option<AppError>,
    will_create_audit_log: bool,
    previous_parent_id: String,
}

async fn new_member_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("you don't have permission to add members to this family"),
        )
        .await;
    }

    let all_raw = attempt!(s, members.all_for_family(family.id).await);
    let mut available_members = Vec::new();
    for raw in all_raw {
        if let Ok(m) = members.materialize(raw).await {
            available_members.push(m);
        }
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    okay(render_template(NewMemberTemplate {
        current_user: Some(user),
        family,
        available_members,
        error: None,
        will_create_audit_log,
        previous_parent_id: String::new(),
    }))
}

#[derive(Deserialize)]
struct NewMemberFormData {
    parent_id: Uuid,
}

async fn new_member_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
    Form(form): Form<NewMemberFormData>,
) -> (StatusCode, Response) {
    get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!(
            "/language-families/{}/members/{}/add-child",
            family.code, form.parent_id
        ))
        .into_response(),
    )
}
