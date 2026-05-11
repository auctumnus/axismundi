#[allow(clippy::unnecessary_wraps)]
pub fn render_md(input: &str) -> Result<String, sqlx::Error> {
    
    let ast = comrak::markdown_to_html(input, &comrak::Options {
        extension: comrak::options::Extension {
            strikethrough: true,
            inline_footnotes: true,
            footnotes: true,
            ..Default::default()
        },
        ..Default::default()
    });

    let sanitized = ammonia::clean(&ast);

    Ok(sanitized)
}
