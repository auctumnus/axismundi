use askama::Template;

use crate::{
    model::{languages::Language, words::WordWithMeta},
    util::ListHeaderKind,
};

#[derive(Template)]
#[template(path = "words/fragments/card.html")]
pub struct PreviewCard<'a> {
    pub word_with_meta: WordWithMeta,
    pub back_url: &'a str,
}

#[derive(Template)]
#[template(path = "words/fragments/list_header.html")]
pub struct Header<'a> {
    pub can_edit_language: bool,
    pub language: &'a Language,
    pub kind: ListHeaderKind,
}

impl Header<'_> {
    fn title(&self) -> &'static str {
        match self.kind {
            ListHeaderKind::Preview => "words",
            ListHeaderKind::Search => "search words",
        }
    }
}
