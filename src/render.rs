//! Baseline-aware stroke layout and tiny-skia rasterization.
use crate::parser::Node;
use serde::Deserialize;
use std::{collections::HashMap, fmt, path::Path};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Debug, Deserialize)]
pub struct StrokeFile {
    schema: String,
    version: u32,
    glyphs: Vec<Glyph>,
}
#[derive(Debug, Deserialize)]
struct Glyph {
    key: String,
    status: String,
    bbox: Option<Bounds>,
    #[serde(default)]
    strokes: Vec<Vec<Point>>,
    #[serde(default)]
    variants: Vec<Variant>,
}
#[derive(Debug, Deserialize)]
struct Variant {
    #[serde(default)]
    baseline: f32,
    bbox: Option<Bounds>,
    strokes: Vec<Vec<Point>>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}
#[derive(Debug, Deserialize)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Debug)]
pub struct RenderError(pub String);
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RenderError {}
type Result<T> = std::result::Result<T, RenderError>;

pub struct Handwriting {
    glyphs: HashMap<String, Glyph>,
}
impl Handwriting {
    pub fn load(path: &Path) -> Result<Self> {
        let size = std::fs::metadata(path)
            .map_err(|e| RenderError(format!("{}: {e}", path.display())))?
            .len();
        if size > 50_000_000 {
            return Err(RenderError("stroke file exceeds 50 MB".into()));
        }
        let contents = std::fs::read_to_string(path)
            .map_err(|e| RenderError(format!("{}: {e}", path.display())))?;
        let file: StrokeFile = serde_json::from_str(&contents)
            .map_err(|e| RenderError(format!("stroke JSON: {e}")))?;
        if file.schema != "aspectwrite.handwriting" || ![1, 2].contains(&file.version) {
            return Err(RenderError(
                "expected aspectwrite.handwriting stroke file version 1 or 2".into(),
            ));
        }
        let mut glyphs = HashMap::new();
        for glyph in file.glyphs {
            if glyphs.insert(glyph.key.clone(), glyph).is_some() {
                return Err(RenderError("duplicate glyph key".into()));
            }
        }
        Ok(Self { glyphs })
    }
    fn lookup(&self, key: &str) -> Result<&Glyph> {
        let key = match key {
            "\\to" | "\\longrightarrow" | "\\xrightarrow" => "\\rightarrow",
            "\\longleftarrow" | "\\xleftarrow" => "\\leftarrow",
            "\\xleftrightarrow" => "\\leftrightarrow",
            "\\Longrightarrow" => "\\Rightarrow",
            "\\ast" => "*",
            "\\prime" => "'",
            _ => key,
        };
        let g = self.glyphs.get(key).ok_or_else(|| {
            RenderError(format!(
                "missing handwriting glyph {key}; collect it or choose an expression not using it"
            ))
        })?;
        if g.status != "complete"
            || (g.variants.is_empty() && (g.bbox.is_none() || g.strokes.is_empty()))
        {
            return Err(RenderError(format!(
                "handwriting glyph {key} has no complete stroke sample"
            )));
        }
        Ok(g)
    }
}

const UNIT: f32 = 0.48; // 145 source-pixel capital -> 70 output pixels
const GAP: f32 = 5.0;
// Geometry may shrink for scripts and fractions, but ink width never does.
const INK_WIDTH: f32 = 2.3;
#[derive(Clone)]
struct Mark {
    points: Vec<(f32, f32)>,
    parenthesis: bool,
}
#[derive(Clone)]
struct Box2 {
    width: f32,
    above: f32,
    below: f32,
    marks: Vec<Mark>,
    large: bool,
}
impl Box2 {
    fn empty() -> Self {
        Self {
            width: 0.0,
            above: 0.0,
            below: 0.0,
            marks: vec![],
            large: false,
        }
    }
    fn translated(mut self, x: f32, y: f32) -> Self {
        for mark in &mut self.marks {
            for p in &mut mark.points {
                p.0 += x;
                p.1 += y;
            }
        }
        self
    }
    fn scaled(mut self, factor: f32) -> Self {
        self.width *= factor;
        self.above *= factor;
        self.below *= factor;
        for mark in &mut self.marks {
            for p in &mut mark.points {
                p.0 *= factor;
                p.1 *= factor;
            }
        }
        self
    }
    fn add(&mut self, other: Self) {
        self.marks.extend(other.marks);
    }
    fn line(&mut self, points: Vec<(f32, f32)>) {
        self.marks.push(Mark {
            points,
            parenthesis: false,
        });
    }
    fn parenthesis(&mut self, points: Vec<(f32, f32)>) {
        self.marks.push(Mark {
            points,
            parenthesis: true,
        });
    }
}

// Subtle deterministic asymmetry: neighboring brackets don't look stamped,
// while repeated renders of the same expression produce identical pixels.
fn vary_parenthesis(mark: &mut Mark) {
    if !mark.parenthesis || mark.points.len() < 3 {
        return;
    }
    let (x, y) = mark.points[0];
    let belly = 1.25 * (x * 0.129 + y * 0.217).sin();
    let skew = 0.95 * (x * 0.071 + y * 0.17).cos();
    let rise = 0.65 * ((x + y) * 0.09).sin();
    let last = (mark.points.len() - 1) as f32;
    for (index, point) in mark.points.iter_mut().enumerate() {
        let t = index as f32 / last;
        let envelope = (std::f32::consts::PI * t).sin();
        point.0 += envelope * (belly + skew * (2.0 * t - 1.0));
        point.1 += envelope * rise;
    }
}

pub fn png(node: &Node, handwriting: &Handwriting) -> Result<Vec<u8>> {
    png_with_seed(node, handwriting, 0)
}

pub fn png_with_seed(node: &Node, handwriting: &Handwriting, seed: u64) -> Result<Vec<u8>> {
    let mut engine = Layout {
        hand: handwriting,
        seed,
        occurrences: HashMap::new(),
    };
    let layout = engine.layout(node)?;
    let margin = 22.0;
    let w = (layout.width + margin * 2.0).ceil().max(1.0);
    let h = (layout.above + layout.below + margin * 2.0).ceil().max(1.0);
    if !w.is_finite() || !h.is_finite() || w > 16000.0 || h > 16000.0 || w * h > 40_000_000.0 {
        return Err(RenderError("image dimensions exceed safe limit".into()));
    }
    let mut pixmap = Pixmap::new(w as u32, h as u32)
        .ok_or_else(|| RenderError("failed to create image".into()))?;
    pixmap.fill(Color::WHITE);
    for mut mark in layout.marks {
        if mark.points.is_empty() {
            continue;
        }
        vary_parenthesis(&mut mark);
        let mut builder = PathBuilder::new();
        if mark.points.len() == 1 {
            let (x, y) = mark.points[0];
            builder.push_circle(margin + x, margin + layout.above + y, INK_WIDTH / 2.0);
            if let Some(path) = builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(Color::BLACK);
                paint.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        } else {
            let (x, y) = mark.points[0];
            builder.move_to(margin + x, margin + layout.above + y);
            for (x, y) in mark.points.into_iter().skip(1) {
                builder.line_to(margin + x, margin + layout.above + y);
            }
            if let Some(path) = builder.finish() {
                let stroke = Stroke {
                    width: INK_WIDTH,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Stroke::default()
                };
                let mut paint = Paint::default();
                paint.set_color(Color::BLACK);
                paint.anti_alias = true;
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
    }
    pixmap
        .encode_png()
        .map_err(|e| RenderError(format!("PNG encoding failed: {e}")))
}

struct Layout<'a> {
    hand: &'a Handwriting,
    seed: u64,
    occurrences: HashMap<String, usize>,
}
impl Layout<'_> {
    /// Relations and fraction bars belong on the user's math axis, not the
    /// writing baseline. Measure it from the center of their '=' sample.
    fn math_axis(&self) -> f32 {
        self.hand
            .glyphs
            .get("=")
            .and_then(|g| g.bbox.as_ref())
            .map_or(34.0, |bb| {
                ((bb.min_y + bb.max_y) * 0.5 * UNIT).clamp(16.0, 60.0)
            })
    }

    fn operator_padding(node: &Node, previous: Option<&Node>) -> f32 {
        match node {
            Node::Arrow(..) => 19.0,
            Node::Glyph(key) => match key.as_str() {
                "=" | "<" | ">" | "\\ne" | "\\equiv" | "\\approx" | "\\simeq"
                | "\\sim" | "\\propto" | "\\le" | "\\ge" | "\\ll" | "\\gg"
                | "\\in" | "\\notin" | "\\mid" | "\\parallel"
                | "\\to" | "\\rightarrow" | "\\longrightarrow" | "\\leftarrow"
                | "\\longleftarrow" | "\\leftrightarrow" | "\\Rightarrow"
                | "\\Longrightarrow" | "\\Leftarrow" | "\\Leftrightarrow"
                | "\\mapsto" | "\\uparrow" | "\\downarrow"
                | "\\rightleftharpoons" => 16.0,
                "+" | "*" | "/" | "\\times" | "\\cdot" | "\\div"
                | "\\pm" | "\\mp" | "\\ast" | "\\oplus" | "\\otimes" => 12.0,
                "-" if previous.is_some_and(|p| !matches!(p, Node::Glyph(g) if matches!(g.as_str(), "=" | "+" | "-" | "<" | ">"))) => 12.0,
                _ => 0.0,
            },
            _ => 0.0,
        }
    }

    fn delimiter_gap(previous: Option<&Node>, next: &Node) -> f32 {
        match (previous, next) {
            (Some(Node::Delimited(..)), Node::Delimited(..)) => 20.0,
            (Some(Node::Glyph(a)), Node::Glyph(b))
                if matches!(a.as_str(), ")" | "]" | "|")
                    && matches!(b.as_str(), "(" | "[" | "|") =>
            {
                10.0
            }
            _ => 0.0,
        }
    }

    fn glyph(&mut self, key: &str) -> Result<Box2> {
        // Literal delimiters are procedural too: no parenthesis/bracket/bar
        // was requested from the collector, even at a fixed height.
        if ["(", ")", "[", "]", "|"].contains(&key) {
            let mut out = Box2 {
                width: 18.0,
                above: 49.0,
                below: 19.0,
                marks: vec![],
                large: false,
            };
            self.delim(
                &mut out,
                key,
                if key == "(" || key == "[" { 13.0 } else { 5.0 },
                49.0,
                19.0,
                key == "(" || key == "[",
            );
            return Ok(out);
        }
        if key == "\\cdots" || key == "\\ldots" {
            let dot = self.glyph(".")?;
            let mut out = Box2::empty();
            out.width = 45.0;
            out.above = if key == "\\cdots" { 31.0 } else { dot.above };
            out.below = dot.below;
            for i in 0..3 {
                out.add(
                    dot.clone()
                        .translated(i as f32 * 17.0, if key == "\\cdots" { -31.0 } else { 0.0 }),
                );
            }
            return Ok(out);
        }
        if key == "\\iint" || key == "\\iiint" {
            let single = self.glyph("\\int")?;
            let n = if key == "\\iint" { 2 } else { 3 };
            let mut out = Box2 {
                width: single.width + (n - 1) as f32 * (single.width * 0.65),
                above: single.above,
                below: single.below,
                marks: vec![],
                large: true,
            };
            for i in 0..n {
                out.add(
                    single
                        .clone()
                        .translated(i as f32 * single.width * 0.65, 0.0),
                );
            }
            return Ok(out);
        }
        let g = self.hand.lookup(key)?;
        let index = self.occurrences.entry(key.to_owned()).or_default();
        let chosen = if g.variants.is_empty() {
            None
        } else {
            Some(&g.variants[(*index + (self.seed as usize % g.variants.len())) % g.variants.len()])
        };
        *index += 1;
        let bb = chosen
            .and_then(|v| v.bbox.as_ref())
            .or(g.bbox.as_ref())
            .ok_or_else(|| RenderError(format!("missing bounding box for glyph {key}")))?;
        let strokes = chosen.map_or(g.strokes.as_slice(), |v| v.strokes.as_slice());
        if chosen.is_some_and(|v| v.baseline != 0.0) || strokes.is_empty() {
            return Err(RenderError(format!("invalid variant for glyph {key}")));
        }
        if ![bb.min_x, bb.max_x, bb.min_y, bb.max_y]
            .iter()
            .all(|v| v.is_finite())
            || bb.max_x < bb.min_x
            || bb.max_y < bb.min_y
        {
            return Err(RenderError(format!("invalid bounding box for glyph {key}")));
        }
        let mut out = Box2 {
            width: ((bb.max_x - bb.min_x) * UNIT).max(3.0) + GAP,
            above: (bb.max_y * UNIT).max(0.0),
            below: (-bb.min_y * UNIT).max(0.0),
            marks: vec![],
            large: matches!(key, "\\int" | "\\oint" | "\\sum" | "\\prod"),
        };
        for stroke in strokes {
            if stroke.is_empty() {
                continue;
            }
            let points: Vec<_> = stroke
                .iter()
                .map(|p| ((p.x - bb.min_x) * UNIT, -p.y * UNIT))
                .collect();
            if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
                return Err(RenderError(format!("invalid stroke point in {key}")));
            }
            out.marks.push(Mark {
                points,
                parenthesis: false,
            });
        }
        Ok(out)
    }
    fn layout(&mut self, n: &Node) -> Result<Box2> {
        match n {
            Node::Glyph(key) => self.glyph(key),
            Node::Text(text) => self.text(text),
            Node::Space(w) => Ok(Box2 {
                width: *w,
                ..Box2::empty()
            }),
            Node::Row(nodes) => {
                let mut out = Box2::empty();
                for (i, node) in nodes.iter().enumerate() {
                    let b = self.layout(node)?;
                    let previous = i.checked_sub(1).map(|j| &nodes[j]);
                    let pad = Self::operator_padding(node, previous);
                    let group_gap = Self::delimiter_gap(previous, node);
                    out.above = out.above.max(b.above);
                    out.below = out.below.max(b.below);
                    let x = out.width + pad + group_gap;
                    out.width = x + b.width + pad;
                    out.add(b.translated(x, 0.0));
                }
                Ok(out)
            }
            Node::Fraction(a, b) => {
                let top = self.layout(a)?.scaled(0.83);
                let bottom = self.layout(b)?.scaled(0.83);
                let width = top.width.max(bottom.width) + 20.0;
                let axis = -self.math_axis();
                let (ty, by) = (axis - 10.0 - top.below, axis + 10.0 + bottom.above);
                let mut out = Box2 {
                    width,
                    above: (-ty + top.above).max(0.0),
                    below: (by + bottom.below).max(0.0),
                    marks: vec![],
                    large: false,
                };
                let tx = (width - top.width) / 2.0;
                let bx = (width - bottom.width) / 2.0;
                out.add(top.translated(tx, ty));
                out.add(bottom.translated(bx, by));
                out.line(vec![(4.0, axis), (width - 4.0, axis)]);
                Ok(out)
            }
            Node::Root(index, body) => {
                let body = self.layout(body)?;
                let cap = body.above.max(56.0) + 9.0;
                let mut out = Box2 {
                    width: body.width + 30.0,
                    above: cap + 4.0,
                    below: body.below.max(13.0),
                    marks: vec![],
                    large: false,
                };
                out.add(body.translated(26.0, 0.0));
                out.line(vec![
                    (1.0, -cap * 0.45),
                    (9.0, -cap * 0.25),
                    (15.0, 10.0),
                    (27.0, -cap),
                    (out.width - 3.0, -cap),
                ]);
                if let Some(index) = index {
                    let small = self.layout(index)?.scaled(0.48);
                    out.above = out.above.max(cap + small.above + 4.0);
                    let x = -small.width * 0.25;
                    out.add(small.translated(x, -cap * 0.75));
                }
                Ok(out)
            }
            Node::Script { base, sub, sup } => {
                let base = self.layout(base)?;
                let sub = sub
                    .as_deref()
                    .map(|n| self.layout(n).map(|b| b.scaled(0.65)))
                    .transpose()?;
                let sup = sup
                    .as_deref()
                    .map(|n| self.layout(n).map(|b| b.scaled(0.65)))
                    .transpose()?;
                if base.large {
                    let width = base
                        .width
                        .max(sup.as_ref().map_or(0.0, |b| b.width))
                        .max(sub.as_ref().map_or(0.0, |b| b.width))
                        + 6.0;
                    // Keep script ink outside the integral's actual ink
                    // bounds, not merely beyond its baseline.
                    let sy = -base.above - 10.0 - sup.as_ref().map_or(0.0, |b| b.below);
                    let uy = base.below + 10.0 + sub.as_ref().map_or(0.0, |b| b.above);
                    let mut out = Box2 {
                        width,
                        above: base.above,
                        below: base.below,
                        marks: vec![],
                        large: false,
                    };
                    let x = (width - base.width) / 2.0;
                    out.add(base.translated(x, 0.0));
                    if let Some(b) = sup {
                        out.above = out.above.max(-sy + b.above);
                        let x = (width - b.width) / 2.0;
                        out.add(b.translated(x, sy));
                    }
                    if let Some(b) = sub {
                        out.below = out.below.max(uy + b.below);
                        let x = (width - b.width) / 2.0;
                        out.add(b.translated(x, uy));
                    }
                    return Ok(out);
                }
                let dx = base.width - 2.0;
                let sy = -base.above.max(34.0) + 10.0;
                let uy = base.below.max(8.0) + 18.0;
                let script_width = sup
                    .as_ref()
                    .map_or(0.0, |b| b.width)
                    .max(sub.as_ref().map_or(0.0, |b| b.width));
                let mut out = Box2 {
                    width: dx + script_width + 5.0,
                    above: base.above,
                    below: base.below,
                    marks: vec![],
                    large: false,
                };
                out.add(base);
                if let Some(b) = sup {
                    out.above = out.above.max(-sy + b.above);
                    out.add(b.translated(dx, sy));
                }
                if let Some(b) = sub {
                    out.below = out.below.max(uy + b.below);
                    out.add(b.translated(dx, uy));
                }
                Ok(out)
            }
            Node::Accent(name, n) => {
                let body = self.layout(n)?;
                let w = body.width;
                let under = body.below + 6.0;
                let y = -body.above - 7.0;
                let mut out = Box2 {
                    width: w,
                    above: body.above + 19.0,
                    below: body.below,
                    marks: vec![],
                    large: false,
                };
                out.add(body);
                match name.as_str() {
                    "hat" | "widehat" => {
                        out.line(vec![(3.0, y), (w / 2.0, y - 10.0), (w - 3.0, y)])
                    }
                    "tilde" | "widetilde" => out.line(vec![
                        (3.0, y - 3.0),
                        (w * 0.3, y - 9.0),
                        (w * 0.7, y),
                        (w - 3.0, y - 6.0),
                    ]),
                    "vec" => {
                        out.line(vec![
                            (3.0, y - 4.0),
                            (w - 4.0, y - 4.0),
                            (w - 12.0, y - 10.0),
                        ]);
                        out.line(vec![(w - 4.0, y - 4.0), (w - 12.0, y + 2.0)]);
                    }
                    "dot" | "ddot" => {
                        for x in if name == "dot" {
                            vec![w / 2.0]
                        } else {
                            vec![w * 0.35, w * 0.65]
                        } {
                            out.line(vec![(x, y - 4.0)]);
                        }
                    }
                    "underline" => {
                        out.line(vec![(2.0, under), (w - 2.0, under)]);
                        out.below += 9.0;
                    }
                    _ => out.line(vec![(2.0, y - 4.0), (w - 2.0, y - 4.0)]),
                }
                Ok(out)
            }
            Node::Delimited(left, n, right) => {
                let b = self.layout(n)?;
                let above = b.above.max(42.0) + 5.0;
                let below = b.below.max(16.0) + 5.0;
                let mut out = Box2 {
                    width: b.width + 30.0,
                    above,
                    below,
                    marks: vec![],
                    large: false,
                };
                out.add(b.translated(15.0, 0.0));
                self.delim(&mut out, left, 5.0, above, below, true);
                let right_x = out.width - 11.0;
                self.delim(&mut out, right, right_x, above, below, false);
                Ok(out)
            }
            Node::Arrow(key, label) => {
                let b = self.layout(label)?.scaled(0.60);
                let arrow_key = match key.as_str() {
                    "\\xrightarrow" => "\\rightarrow",
                    "\\xleftarrow" => "\\leftarrow",
                    _ => "\\leftrightarrow",
                };
                let arrow = self.glyph(arrow_key)?;
                let width = arrow.width.max(b.width + 14.0);
                let y = -arrow.above - b.below - 8.0;
                let mut out = Box2 {
                    width,
                    above: (-y + b.above).max(arrow.above),
                    below: arrow.below,
                    marks: vec![],
                    large: false,
                };
                let arrow_x = (width - arrow.width) / 2.0;
                let label_x = (width - b.width) / 2.0;
                out.add(arrow.translated(arrow_x, 0.0));
                out.add(b.translated(label_x, y));
                Ok(out)
            }
            Node::Aligned(rows) => {
                let layouts = rows
                    .iter()
                    .map(|(a, b)| {
                        Ok((
                            self.layout(a)?,
                            b.as_ref().map(|b| self.layout(b)).transpose()?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let left_w = layouts.iter().map(|(a, _)| a.width).fold(0.0, f32::max);
                let right_w = layouts
                    .iter()
                    .filter_map(|(_, b)| b.as_ref().map(|b| b.width))
                    .fold(0.0, f32::max);
                let mut out = Box2::empty();
                let mut cursor = 0.0;
                for (i, (a, b)) in layouts.into_iter().enumerate() {
                    let above = a.above.max(b.as_ref().map_or(0.0, |b| b.above));
                    let below = a.below.max(b.as_ref().map_or(0.0, |b| b.below));
                    cursor += if i == 0 { above } else { above + 17.0 };
                    let baseline = cursor;
                    let x = left_w - a.width;
                    out.add(a.translated(x, baseline));
                    if let Some(b) = b {
                        out.add(b.translated(left_w + 12.0, baseline));
                    }
                    cursor += below;
                }
                out.width = left_w + if right_w > 0.0 { 12.0 + right_w } else { 0.0 };
                out.above = 0.0;
                out.below = cursor;
                Ok(out)
            }
        }
    }
    fn text(&mut self, text: &str) -> Result<Box2> {
        let mut out = Box2::empty();
        for ch in text.chars() {
            let b = if ch == ' ' {
                Box2 {
                    width: 32.0,
                    ..Box2::empty()
                }
            } else if ch == '%' {
                self.glyph("\\%")?
            } else {
                self.glyph(&ch.to_string())?
            };
            out.above = out.above.max(b.above);
            out.below = out.below.max(b.below);
            let x = out.width;
            out.width += b.width;
            out.add(b.translated(x, 0.0));
        }
        Ok(out)
    }
    fn delim(&self, out: &mut Box2, token: &str, x: f32, up: f32, down: f32, opening: bool) {
        if token == "." {
            return;
        }
        let top = -up;
        let bot = down;
        let mid = (top + bot) / 2.0;
        let kind = match token {
            "(" | ")" => "round",
            "[" | "]" => "square",
            "|" | "\\lvert" | "\\rvert" => "bar",
            "\\langle" | "\\rangle" => "angle",
            _ => "brace",
        };
        match kind {
            "round" => {
                let mut pts = Vec::new();
                for i in 0..=20 {
                    let t = i as f32 / 20.0;
                    let y = top + (bot - top) * t;
                    let bulge = (1.0 - (2.0 * t - 1.0).powi(2)).max(0.0) * 8.0;
                    pts.push((x + if opening { -bulge } else { bulge }, y));
                }
                out.parenthesis(pts);
            }
            "square" => {
                let d = if opening { -7.0 } else { 7.0 };
                out.line(vec![(x, top), (x + d, top), (x + d, bot), (x, bot)]);
            }
            "bar" => out.line(vec![(x, top), (x, bot)]),
            "angle" => out.line(vec![
                (x, top),
                (x + if opening { -8.0 } else { 8.0 }, mid),
                (x, bot),
            ]),
            _ => {
                let d = if opening { -8.0 } else { 8.0 };
                out.line(vec![
                    (x, top),
                    (x + d, top + 5.0),
                    (x + d, mid - 5.0),
                    (x, mid),
                    (x + d, mid + 5.0),
                    (x + d, bot - 5.0),
                    (x, bot),
                ]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn fixture() -> Handwriting {
        let sample = |key: &str, max_y: f32, min_y: f32| {
            serde_json::json!({
                "key": key, "status": "complete",
                "bbox": {"minX": 0.0, "maxX": 45.0, "minY": min_y, "maxY": max_y},
                "strokes": [[{"x": 0.0, "y": max_y}, {"x": 45.0, "y": min_y}]]
            })
        };
        let file: StrokeFile = serde_json::from_value(serde_json::json!({
            "schema": "aspectwrite.handwriting", "version": 1,
            "glyphs": [sample("=", 87.0, 55.0), sample("x", 65.0, 0.0),
                       sample("1", 120.0, 0.0), sample("T", 125.0, 0.0),
                       sample("\\int", 130.0, -5.0)]
        }))
        .unwrap();
        Handwriting {
            glyphs: file
                .glyphs
                .into_iter()
                .map(|g| (g.key.clone(), g))
                .collect(),
        }
    }

    #[test]
    fn fraction_bar_tracks_equal_sign_math_axis() {
        let hand = fixture();
        let mut layout = Layout {
            hand: &hand,
            seed: 0,
            occurrences: HashMap::new(),
        };
        let fraction = layout.layout(&parse(r"\frac{x}{x}").unwrap()).unwrap();
        let bar = fraction
            .marks
            .iter()
            .find(|m| m.points.first().is_some_and(|p| p.0 == 4.0))
            .unwrap();
        let equals = layout.glyph("=").unwrap();
        let equals_center = (equals.marks[0].points[0].1 + equals.marks[0].points[1].1) / 2.0;
        assert!((bar.points[0].1 - equals_center).abs() < 0.1);
        assert!(fraction.below > 0.0); // denominator can descend below the baseline
    }

    #[test]
    fn relation_and_reaction_spacing() {
        assert_eq!(
            Layout::operator_padding(&Node::Glyph("=".into()), None),
            16.0
        );
        assert_eq!(
            Layout::operator_padding(&Node::Glyph("\\rightarrow".into()), None),
            16.0
        );
        assert_eq!(
            Layout::operator_padding(
                &Node::Arrow("\\xrightarrow".into(), Box::new(Node::Row(vec![]))),
                None
            ),
            19.0
        );
    }

    #[test]
    fn neighboring_parentheses_have_visible_ink_gap() {
        let hand = fixture();
        let mut layout = Layout {
            hand: &hand,
            seed: 0,
            occurrences: HashMap::new(),
        };
        for expression in [r"\left(x\right)\left(x\right)", ")("] {
            let result = layout.layout(&parse(expression).unwrap()).unwrap();
            let curves: Vec<_> = result
                .marks
                .iter()
                .filter(|m| m.points.len() == 21)
                .collect();
            assert!(curves.len() >= 2);
            // For scalable groups: left, right, left, right. For literal
            // delimiters: right, left. In either case, compare the inner pair.
            let (right, left) = if curves.len() == 4 {
                (curves[1], curves[2])
            } else {
                (curves[0], curves[1])
            };
            let max_right = right
                .points
                .iter()
                .map(|p| p.0)
                .fold(f32::NEG_INFINITY, f32::max);
            let min_left = left
                .points
                .iter()
                .map(|p| p.0)
                .fold(f32::INFINITY, f32::min);
            assert!(
                min_left - max_right >= 18.0,
                "{expression}: gap = {}",
                min_left - max_right
            );
        }
    }

    #[test]
    fn parenthesis_variation_is_subtle_distinct_and_repeatable() {
        let hand = fixture();
        let result = Layout {
            hand: &hand,
            seed: 0,
            occurrences: HashMap::new(),
        }
        .layout(&parse(r"\left(x\right)\left(x\right)").unwrap())
        .unwrap();
        let mut curves = result.marks.iter().filter(|m| m.parenthesis);
        let mut first = curves.next().unwrap().clone();
        curves.next(); // closing parenthesis
        let mut second = curves.next().unwrap().clone();
        let original = first.clone();
        let second_original = second.clone();
        let mut repeat = first.clone();
        vary_parenthesis(&mut first);
        vary_parenthesis(&mut second);
        vary_parenthesis(&mut repeat);
        assert_eq!(first.points, repeat.points);
        assert_eq!(first.points[0], original.points[0]);
        assert_eq!(first.points.last(), original.points.last());
        let middle = first.points.len() / 2;
        let first_shift = first.points[middle].0 - original.points[middle].0;
        let second_shift = second.points[middle].0 - second_original.points[middle].0;
        assert!((first_shift - second_shift).abs() > 0.01);
        assert!(first_shift.abs() <= 2.2);
        assert!(second_shift.abs() <= 2.2);
    }

    #[test]
    fn integral_limits_clear_ink_and_prose_has_word_gaps() {
        let hand = fixture();
        let mut layout = Layout {
            hand: &hand,
            seed: 0,
            occurrences: HashMap::new(),
        };
        let integral = layout.glyph("\\int").unwrap();
        let limits = layout.layout(&parse(r"\int_{T_1}^{T_1}").unwrap()).unwrap();
        assert!(limits.above > integral.above + 10.0);
        assert!(limits.below > integral.below + 10.0);
        let one_word = layout.text("xx").unwrap();
        let two_words = layout.text("x x").unwrap();
        assert!((two_words.width - one_word.width - 32.0).abs() < 0.1);
    }
}
