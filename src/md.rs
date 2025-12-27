use markdown_it::{
    Node, NodeValue, Renderer,
    parser::inline::{InlineRule, InlineState},
    plugins::sourcepos,
};
use sqlx::PgPool;

use crate::{md, model::users::USERNAME_REGEX};

#[derive(Debug)]
pub struct UserMention {
    pub username: String,
    pub is_linked: bool,
}

// This defines how your custom node should be rendered.
impl NodeValue for UserMention {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        if self.is_linked {
            let mut attrs = node.attrs.clone();

            // add a custom class attribute
            attrs.push(("href", format!("/users/{}", self.username)));
            attrs.push(("class", "user-mention".into()));

            fmt.open("a", &attrs);
            fmt.text(&format!("@{}", self.username));
            fmt.close("a");
        } else {
            let mut attrs = node.attrs.clone();
            attrs.push(("class", "user-mention".into()));

            fmt.open("span", &attrs);
            fmt.text(&format!("@{}", self.username));
            fmt.close("span");
        }
    }
}

// This is an extension for the inline subparser.
struct UserMentionScanner;

impl InlineRule for UserMentionScanner {
    const MARKER: char = '@';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let input = &state.src[state.pos..state.pos_max]; // look for stuff at state.pos
        if !input.starts_with('@') {
            return None;
        } // return None if it's not found

        // TODO: seems DoSy
        let caps = USERNAME_REGEX.captures(input)?;
        let username = caps.get(1).unwrap().as_str().to_string();
        let len = username.len() + 1;

        Some((
            Node::new(UserMention {
                username,
                is_linked: true,
            }),
            len,
        ))
    }
}

// TODO: i Really Really want to check that users are real
// users before linking to them. but that requires async db access
// and markdown-it stores nodes with Rc<>s inside which is gughhhhghghhghh
// evil ass ____ building
pub fn render_md(input: &str) -> Result<String, sqlx::Error> {
    let md = &mut markdown_it::MarkdownIt::new();

    markdown_it::plugins::cmark::add(md);

    md.inline.add_rule::<UserMentionScanner>();

    let mut ast = md.parse(input);

    Ok(ast.render())
}
