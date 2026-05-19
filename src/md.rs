use std::sync::Arc;

use crate::util::s3::S3;

#[allow(clippy::unnecessary_wraps)]
pub fn render_md(input: &str) -> Result<String, sqlx::Error> {
    let ast = comrak::markdown_to_html(input, &comrak::Options {
        extension: comrak::options::Extension {
            strikethrough: true,
            inline_footnotes: true,
            footnotes: true,
            image_url_rewriter: Some(Arc::new(|url: &str| S3.get_external_url(url))),
            ..Default::default()
        },
        ..Default::default()
    });

    let sanitized = ammonia::clean(&ast);

    Ok(sanitized)
}
