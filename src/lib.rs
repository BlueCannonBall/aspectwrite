pub mod parser;
pub mod render;
pub mod server;

/// Parse supported LaTeX, lay out the collected handwriting, and encode a PNG.
pub fn render_png(
    latex: &str,
    strokes: &render::Handwriting,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    render_png_with_seed(latex, strokes, 0)
}

pub fn render_png_with_seed(
    latex: &str,
    strokes: &render::Handwriting,
    seed: u64,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if latex.len() > 8192 {
        return Err("LaTeX exceeds 8192 bytes".into());
    }
    let ast = parser::parse(latex)?;
    Ok(render::png_with_seed(&ast, strokes, seed)?)
}
