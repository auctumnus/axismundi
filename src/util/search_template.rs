use std::fmt::Display;

use crate::{
    err::AppError, model::users::User, pagination::{PaginatedRequest, PaginatedResponse, PaginationTemplate}, util::{EmptyTemplate, HasTextQuery}
};
use askama::Template;
use serde::Serialize;

#[derive(Template)]
#[template(path = "search_layout.html")]
pub struct SearchTemplate<
    T,
    Header: Template,
    Query: HasTextQuery,
    QueryTemplate: Template,
    Item: Template,
    ItemRenderer: Fn(&T) -> Item,
    SearchName: Display, 
    SearchAction: Display,
    Breadcrumbs: Template = EmptyTemplate,
    Footer: Template = EmptyTemplate,
> {
    pub current_user: Option<User>,
    pub header: Header,
    pub query: Query,
    pub results: Option<PaginatedResponse<T>>,
    pub pagination: PaginationTemplate,
    pub error: Option<AppError>,
    pub search_name: SearchName,
    pub search_action: SearchAction,
    pub render_item: ItemRenderer,
    pub query_template: QueryTemplate,
    pub breadcrumbs: Option<Breadcrumbs>,
    pub footer: Option<Footer>,
}

pub struct SearchTemplateArgs<
    T,
    Header: Template,
    Query: HasTextQuery,
    QueryTemplate: Template,
    Item: Template,
    ItemRenderer: Fn(&T) -> Item,
    SearchName: Display, 
    SearchAction: Display,
> {
    pub current_user: Option<User>,
    pub header: Header,
    pub query: Query,
    pub results: Result<PaginatedResponse<T>, AppError>,
    pub pagination: PaginatedRequest,
    pub search_name: SearchName,
    pub search_action: SearchAction,
    pub render_item: ItemRenderer,
    pub query_template: QueryTemplate,
}

pub fn make_search_layout<
    T,
    Header: Template,
    Query: HasTextQuery + Serialize,
    QueryTemplate: Template,
    Item: Template,
    ItemRenderer: Fn(&T) -> Item,
    SearchName: Display, 
    SearchAction: Display,
>(layout: SearchTemplateArgs<T, Header, Query, QueryTemplate, Item, ItemRenderer, SearchName, SearchAction>) -> SearchTemplate<T, Header, Query, QueryTemplate, Item, ItemRenderer, SearchName, SearchAction, EmptyTemplate, EmptyTemplate> {
    let SearchTemplateArgs {
        current_user,
        header,
        query,
        results,
        pagination,
        search_name,
        search_action,
        render_item,
        query_template,
    } = layout;
    match results {
        Ok(res) => {
            let pagination = PaginationTemplate::from_paginated_response(search_action.to_string().as_str(), &res, &pagination, &query);
            SearchTemplate {
                current_user,
                header,
                query,
                results: Some(res),
                pagination,
                error: None,
                search_name,
                search_action,
                render_item,
                query_template,
                breadcrumbs: None,
                footer: None,
            }
        },
        Err(e) => {
            let pagination = PaginationTemplate::from_error(search_action.to_string().as_str(), &pagination, &query);
            SearchTemplate {
                current_user,
                header,
                query,
                query_template,
                results: None,
                pagination,
                error: Some(e),
                search_name,
                search_action,
                render_item,
                breadcrumbs: None,
                footer: None,
            }
        }
    }
}

impl<
    T,
    Header: Template,
    Query: HasTextQuery + Serialize,
    QueryTemplate: Template,
    Item: Template,
    ItemRenderer: Fn(&T) -> Item,
    SearchName: Display, 
    SearchAction: Display,
    Breadcrumbs: Template,
    Footer: Template,
> SearchTemplate<T, Header, Query, QueryTemplate, Item, ItemRenderer, SearchName, SearchAction, Breadcrumbs, Footer> {
    pub fn status(&self) -> axum::http::StatusCode {
        if self.error.is_none() {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }

    pub fn with_breadcrumbs<NewBreadcrumbs: Template>(self, breadcrumbs: NewBreadcrumbs) -> SearchTemplate<T, Header, Query, QueryTemplate, Item, ItemRenderer, SearchName, SearchAction, NewBreadcrumbs, Footer> {
        let SearchTemplate {
            current_user,
            header,
            query,
            results,
            pagination,
            error,
            search_name,
            search_action,
            render_item,
            footer,
            query_template,
            ..
        } = self;
        SearchTemplate {
            breadcrumbs: Some(breadcrumbs),
            header,
            query_template,
            current_user,
            query,
            results,
            pagination,
            error,
            search_name,
            search_action,
            render_item,
            footer,
        }
    }

    pub fn with_footer<NewFooter: Template>(self, footer: NewFooter) -> SearchTemplate<T, Header, Query, QueryTemplate, Item, ItemRenderer, SearchName, SearchAction, Breadcrumbs, NewFooter> {
        let SearchTemplate {
            query_template,
            header,
            current_user,
            query,
            results,
            pagination,
            error,
            search_name,
            search_action,
            render_item,
            breadcrumbs,
            ..
        } = self;
        SearchTemplate {
            footer: Some(footer),
            header,
            query_template,
            current_user,
            query,
            results,
            pagination,
            error,
            search_name,
            search_action,
            render_item,
            breadcrumbs,
        }
    }
}