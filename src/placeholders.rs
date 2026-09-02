//! `%%{…}` preprocessor directives, shared by grammar tables and IPA
//! estimators.
//!
//! Values are inserted as source text rather than escaped, so authors can use
//! them anywhere Lexurgy syntax accepts text.  A directive that cannot be
//! resolved is an error: quietly substituting an empty string would produce
//! source that parses and then silently runs the wrong rules.

use serde_json::Value;

use crate::model::words::Word;

/// The word data a `%%{…}` directive can reach.  Every field is optional
/// because the directives run in places that know less than a saved word: a
/// grammar-table preview may receive only a spelling, and an IPA estimator run
/// over free text knows only the token it is estimating.
#[derive(Debug, Clone, Copy, Default)]
pub struct Placeholders<'a> {
    pub word: Option<&'a str>,
    pub ipa: Option<&'a str>,
    pub extra: Option<&'a Value>,
}

impl<'a> Placeholders<'a> {
    /// Everything a saved word carries.
    pub fn for_word(word: &'a Word) -> Self {
        Self {
            word: Some(&word.word),
            ipa: Some(&word.ipa),
            extra: word.extra.as_ref(),
        }
    }

    /// Just a spelling, for previews and other unsaved input.
    pub fn for_spelling(word: &'a str) -> Self {
        Self {
            word: Some(word),
            ..Self::default()
        }
    }

    pub fn with_ipa(self, ipa: Option<&'a str>) -> Self {
        Self { ipa, ..self }
    }

    pub fn with_extra(self, extra: Option<&'a Value>) -> Self {
        Self { extra, ..self }
    }

    pub fn with_word(self, word: &'a str) -> Self {
        Self {
            word: Some(word),
            ..self
        }
    }
}

/// Cheap check for whether expansion could change `source` at all.
pub fn contains_directive(source: &str) -> bool {
    source.contains("%%{")
}

fn value_as_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".into(),
        value @ (Value::Array(_) | Value::Object(_)) => value.to_string(),
    }
}

fn directive_value(path: &str, values: &Placeholders<'_>) -> Result<String, String> {
    let missing = |path: &str| format!("The placeholder `%%{{{path}}}` has no value here.");
    match path {
        "word" => values
            .word
            .map(str::to_owned)
            .ok_or_else(|| missing("word")),
        "ipa" => values.ipa.map(str::to_owned).ok_or_else(|| missing("ipa")),
        _ => {
            let Some(path) = path.strip_prefix("extra.") else {
                return Err(format!("Unknown placeholder `%%{{{path}}}`."));
            };
            if path.is_empty() {
                return Err("Placeholder paths cannot be empty.".into());
            }
            let mut value = values
                .extra
                .ok_or_else(|| missing(&format!("extra.{path}")))?;
            for segment in path.split('.') {
                if segment.is_empty() {
                    return Err("Placeholder paths cannot contain empty segments.".into());
                }
                value = match value {
                    Value::Object(values) => values.get(segment),
                    Value::Array(values) => segment
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| values.get(index)),
                    _ => None,
                }
                .ok_or_else(|| missing(&format!("extra.{path}")))?;
            }
            Ok(value_as_text(value))
        }
    }
}

/// Expand every `%%{…}` directive in `source`.
///
/// Supported paths are `word`, `ipa`, and `extra.path`, where `extra.path`
/// descends through JSON objects by key and arrays by numeric index.
pub fn expand(source: &str, values: &Placeholders<'_>) -> Result<String, String> {
    let mut expanded = String::with_capacity(source.len());
    let mut remainder = source;
    while let Some(start) = remainder.find("%%{") {
        expanded.push_str(&remainder[..start]);
        let directive_start = start + 3;
        let Some(end) = remainder[directive_start..].find('}') else {
            return Err("Unclosed placeholder.".into());
        };
        let end = directive_start + end;
        expanded.push_str(&directive_value(&remainder[directive_start..end], values)?);
        remainder = &remainder[end + 1..];
    }
    expanded.push_str(remainder);
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_word_ipa_and_nested_extra_directives() {
        let extra = serde_json::json!({
            "stem": { "plural": "stem-pl" },
            "classes": ["first", "second"],
            "count": 2,
        });
        let values = Placeholders::for_spelling("spelling")
            .with_ipa(Some("ipa"))
            .with_extra(Some(&extra));

        assert_eq!(
            expand(
                "%%{word} %%{ipa} %%{extra.stem.plural} %%{extra.classes.1} %%{extra.count}",
                &values,
            )
            .unwrap(),
            "spelling ipa stem-pl second 2"
        );
    }

    #[test]
    fn directive_errors_name_the_missing_value() {
        let values = Placeholders::for_spelling("spelling");
        assert!(
            expand("%%{extra.stem}", &values)
                .unwrap_err()
                .contains("%%{extra.stem}")
        );
        assert!(expand("%%{ipa}", &values).unwrap_err().contains("%%{ipa}"));
        assert!(expand("%%{unknown}", &values).is_err());
        assert!(expand("%%{word", &values).is_err());
    }

    #[test]
    fn source_without_directives_is_unchanged() {
        let values = Placeholders::default();
        assert!(!contains_directive("a => b"));
        assert_eq!(expand("a => b", &values).unwrap(), "a => b");
    }
}
