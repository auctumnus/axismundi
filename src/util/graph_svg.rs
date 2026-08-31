use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use vizoxide::{
    Context, Graph,
    attr::{edge, graph, node},
    layout::{Engine, apply_layout},
    render::{Format, render_to_string},
};

use crate::model::language_families::{FamilyRelationKindV1, LanguageFamilySchemaV1};
use crate::model::word_relations::{CognacyRelationKindV1, LeveledCognacy};
use crate::{
    err::{AppResult, internal_error},
    model::word_relations::{RelationDirection, WordRelationType},
};

/// Convert a `LeveledCognacy` to an SVG string.
///
/// Node labels show language names above the word text only in graphs spanning
/// multiple languages.
/// Edge styles indicate relation type:
/// - Derived/Descendant: solid line
/// - Compound: bold line
/// - Calque: dashed line
/// - Borrowed: dotted line
pub fn cognacy_to_svg(
    cognacy: &LeveledCognacy,
    current_word_id: Option<Uuid>,
) -> AppResult<String> {
    let ctx = Context::new()
        .map_err(|e| internal_error(format!("Failed to create graphviz context: {}", e)))?;

    let mut g = Graph::builder("cognacy")
        .directed(true)
        .attribute("class", "cognacy-graph")
        .attribute(graph::RANKDIR, "TB")
        .attribute(graph::BGCOLOR, "transparent")
        .attribute(graph::NODESEP, "0.5")
        .attribute(graph::RANKSEP, "0.75")
        .build()
        .map_err(|e| internal_error(format!("Failed to create graph: {}", e)))?;

    // Create nodes for each word
    let mut node_map: HashMap<Uuid, vizoxide::Node> = HashMap::new();
    let graph_spans_languages = cognacy
        .words
        .values()
        .map(|word| word.language)
        .collect::<HashSet<_>>()
        .len()
        > 1;

    for word_id in cognacy.words.keys() {
        let word = &cognacy.words[word_id];
        let node_id = word_id.to_string();

        let is_current_word = current_word_id == Some(*word_id);
        let language_label =
            cognacy_language_label(word.language_name.as_deref(), graph_spans_languages);
        let node_class = match (is_current_word, language_label.is_some()) {
            (true, true) => "cognacy-node current-word cognacy-node-language",
            (true, false) => "cognacy-node current-word cognacy-node-word-only",
            (false, true) => "cognacy-node cognacy-node-language",
            (false, false) => "cognacy-node cognacy-node-word-only",
        };

        let word_url = word.language_code.as_deref().map(|lang_code| {
            format!(
                "/languages/{}/words/{}/{}",
                lang_code, word.slug, word.lemma
            )
        });

        let label = cognacy_node_label(&word.word, language_label);

        let mut node_builder = g
            .create_node(&node_id)
            .attribute(node::LABEL, &label)
            .attribute(node::SHAPE, "box")
            .attribute(node::STYLE, "rounded")
            .attribute(node::FONTNAME, "sans-serif")
            .attribute("class", node_class);

        if let Some(url) = &word_url {
            node_builder = node_builder.attribute(graph::URL, url.as_str());
        }

        let node = node_builder
            .build()
            .map_err(|e| internal_error(format!("Failed to create node: {}", e)))?;

        node_map.insert(*word_id, node);
    }

    // Create edges with styling based on relation type
    for edge_data in &cognacy.edges {
        let from_node = node_map
            .get(&edge_data.antecedent)
            .ok_or_else(|| internal_error("Edge references missing antecedent node"))?;
        let to_node = node_map
            .get(&edge_data.consequent)
            .ok_or_else(|| internal_error("Edge references missing consequent node"))?;

        let (style, penwidth) = edge_style_for_cognacy(edge_data.kind);

        g.create_edge(from_node, to_node, None)
            .attribute(edge::STYLE, style)
            .attribute(edge::PENWIDTH, penwidth)
            .attribute("arrowhead", "vee")
            .attribute("arrowsize", "0.75")
            .attribute("class", "cognacy-edge")
            .attribute(
                "tooltip",
                <CognacyRelationKindV1 as Into<WordRelationType>>::into(edge_data.kind)
                    .text(&RelationDirection::Antecedent),
            )
            .build()
            .map_err(|e| internal_error(format!("Failed to create edge: {}", e)))?;
    }

    apply_layout(&ctx, &mut g, Engine::Dot)
        .map_err(|e| internal_error(format!("Failed to apply layout: {}", e)))?;

    let svg = render_to_string(&ctx, &g, Format::Svg)
        .map_err(|e| internal_error(format!("Failed to render SVG: {}", e)))?;

    Ok(svg)
}

/// Show all language labels or none: labels help distinguish a mixed-language
/// graph, but add noise when every word belongs to the same language.
fn cognacy_language_label<'a>(
    language_name: Option<&'a str>,
    graph_spans_languages: bool,
) -> Option<&'a str> {
    if graph_spans_languages {
        language_name.filter(|name| !name.is_empty())
    } else {
        None
    }
}

fn cognacy_node_label(word: &str, language_name: Option<&str>) -> String {
    match language_name {
        Some(name) => format!("{name}\n{word}"),
        _ => word.to_string(),
    }
}

fn edge_style_for_cognacy(kind: CognacyRelationKindV1) -> (&'static str, &'static str) {
    match kind {
        CognacyRelationKindV1::Derived | CognacyRelationKindV1::Descendant => ("solid", "1.0"),
        CognacyRelationKindV1::Compound => ("solid", "2.0"),
        CognacyRelationKindV1::Calque => ("dashed", "1.0"),
        CognacyRelationKindV1::Borrowed => ("dotted", "1.0"),
    }
}

#[cfg(test)]
mod tests {
    use super::{cognacy_language_label, cognacy_node_label};

    #[test]
    fn cognacy_node_label_includes_language_name_above_word() {
        assert_eq!(
            cognacy_node_label("pater", Some("Latin")),
            "Latin\npater"
        );
    }

    #[test]
    fn cognacy_node_label_shows_current_words_language_in_a_multilingual_graph() {
        let language_label = cognacy_language_label(Some("Latin"), true);
        assert_eq!(cognacy_node_label("pater", language_label), "Latin\npater");
    }

    #[test]
    fn cognacy_node_label_omits_languages_in_a_single_language_graph() {
        let language_label = cognacy_language_label(Some("Latin"), false);
        assert_eq!(cognacy_node_label("pater", language_label), "pater");
    }

    #[test]
    fn cognacy_node_label_omits_a_missing_language_name() {
        assert_eq!(cognacy_node_label("pater", None), "pater");
    }
}

pub enum LanguageFamilyMemberLabel {
    Language { name: String, code: String },
    Grouping { notes: String },
}

impl LanguageFamilyMemberLabel {
    fn as_str(&self) -> String {
        match self {
            LanguageFamilyMemberLabel::Language { name, code } => format!("{name} ({code})"),
            LanguageFamilyMemberLabel::Grouping { notes } => notes.to_string(),
        }
    }

    fn get_shape(&self) -> &'static str {
        match self {
            LanguageFamilyMemberLabel::Language { .. } => "box",
            LanguageFamilyMemberLabel::Grouping { .. } => "plain",
        }
    }

    fn get_style(&self) -> &'static str {
        match self {
            LanguageFamilyMemberLabel::Language { .. } => "rounded",
            LanguageFamilyMemberLabel::Grouping { .. } => "",
        }
    }
}

/// Convert a language family tree to an SVG string.
///
/// Node labels show language name or "group" for organizational nodes.
/// Edge styles:
/// - Descendant: solid line
/// - Hybrid: dashed line
pub fn language_family_to_svg(
    family_code: &str,
    schema: &LanguageFamilySchemaV1,
    member_labels: &HashMap<Uuid, LanguageFamilyMemberLabel>,
) -> AppResult<String> {
    let ctx = Context::new()
        .map_err(|e| internal_error(format!("Failed to create graphviz context: {}", e)))?;

    let mut g = Graph::builder("language_family")
        .directed(true)
        .attribute("class", "language-family-graph")
        .attribute(graph::BGCOLOR, "transparent")
        .attribute(graph::RANKDIR, "TB")
        .attribute(graph::NODESEP, "0.5")
        .attribute(graph::RANKSEP, "0.75")
        .build()
        .map_err(|e| internal_error(format!("Failed to create graph: {}", e)))?;

    // Collect all member IDs from edges
    let mut member_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for edge in &schema.edges {
        if let Some(parent_id) = edge.parent_member_id {
            member_ids.insert(parent_id);
        }
        member_ids.insert(edge.child_member_id);
    }

    // Create nodes for each member
    let mut node_map: HashMap<Uuid, vizoxide::Node> = HashMap::new();

    for member_id in member_ids {
        let node_id = member_id.to_string();
        let label = member_labels
            .get(&member_id)
            .map_or("group".to_string(), LanguageFamilyMemberLabel::as_str);

        let node = g
            .create_node(&node_id)
            .attribute(
                graph::URL,
                &format!("/language-families/{}/members/{}", family_code, member_id),
            )
            .attribute("class", "language-family-node")
            .attribute(node::LABEL, &label)
            .attribute(
                node::SHAPE,
                member_labels
                    .get(&member_id)
                    .map_or("box", |s| s.get_shape()),
            )
            .attribute(
                node::STYLE,
                member_labels
                    .get(&member_id)
                    .map_or("rounded", |s| s.get_style()),
            )
            .build()
            .map_err(|e| internal_error(format!("Failed to create node: {}", e)))?;

        node_map.insert(member_id, node);
    }

    // Create edges
    for edge_data in &schema.edges {
        let Some(parent_id) = edge_data.parent_member_id else {
            // Root node, no edge to create
            continue;
        };

        let from_node = node_map
            .get(&parent_id)
            .ok_or_else(|| internal_error("Edge references missing parent node"))?;
        let to_node = node_map
            .get(&edge_data.child_member_id)
            .ok_or_else(|| internal_error("Edge references missing child node"))?;

        let style = edge_style_for_family(edge_data.relation_kind);

        g.create_edge(from_node, to_node, None)
            .attribute(edge::STYLE, style)
            .attribute("arrowhead", "vee")
            .attribute("arrowsize", "0.75")
            .attribute("class", "language-family-edge")
            .build()
            .map_err(|e| internal_error(format!("Failed to create edge: {}", e)))?;
    }

    apply_layout(&ctx, &mut g, Engine::Dot)
        .map_err(|e| internal_error(format!("Failed to apply layout: {}", e)))?;

    let svg = render_to_string(&ctx, &g, Format::Svg)
        .map_err(|e| internal_error(format!("Failed to render SVG: {}", e)))?;

    Ok(svg)
}

fn edge_style_for_family(kind: FamilyRelationKindV1) -> &'static str {
    match kind {
        FamilyRelationKindV1::Descendant => "solid",
        FamilyRelationKindV1::Hybrid => "dashed",
    }
}

/// Helper function to render a family tree from a `LanguageFamily`.
/// Fetches member labels from the database and generates the SVG.
pub async fn render_family_tree(
    family: &crate::model::language_families::LanguageFamily,
    members: &crate::model::language_family_members::LanguageFamilyMemberRepository,
) -> AppResult<String> {
    use crate::model::language_families::LanguageFamilyInner;

    let schema = family.tree_schema()?;

    let LanguageFamilyInner::V1(v1_schema) = schema;

    // Collect all member IDs and fetch their labels
    let mut member_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for edge in &v1_schema.edges {
        if let Some(parent_id) = edge.parent_member_id {
            member_ids.insert(parent_id);
        }
        member_ids.insert(edge.child_member_id);
    }

    // Build member labels map
    let mut member_labels: HashMap<Uuid, LanguageFamilyMemberLabel> = HashMap::new();
    for member_id in member_ids {
        if let Ok(member) = members.find_by_id(member_id).await {
            if let Ok(materialized) = members.materialize(member).await {
                use crate::model::language_family_members::LanguageFamilyMember;
                let label = match &materialized.member {
                    LanguageFamilyMember::Language(_) => {
                        if let Some(lang) = materialized.language {
                            LanguageFamilyMemberLabel::Language {
                                name: lang.name,
                                code: lang.code,
                            }
                        } else {
                            // shouldn't really occur, but fallback to at least having a label
                            LanguageFamilyMemberLabel::Grouping {
                                notes: "(unknown language)".to_string(),
                            }
                        }
                    }
                    LanguageFamilyMember::Grouping(g) => LanguageFamilyMemberLabel::Grouping {
                        notes: g.title.clone(),
                    },
                };
                member_labels.insert(member_id, label);
            }
        }
    }

    language_family_to_svg(&family.code, &v1_schema, &member_labels)
}
