//! Strict recursive-descent parser for docs/latex-subset.md. Offsets are byte offsets.
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Row(Vec<Node>),
    Glyph(String),
    Text(String),
    Space(f32),
    Fraction(Box<Node>, Box<Node>),
    Root(Option<Box<Node>>, Box<Node>),
    Script {
        base: Box<Node>,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    Accent(String, Box<Node>),
    Delimited(String, Box<Node>, String),
    Arrow(String, Box<Node>),
    Aligned(Vec<(Node, Option<Node>)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LaTeX byte {}: {}", self.offset, self.message)
    }
}
impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

pub fn parse(src: &str) -> Result<Node> {
    if let Some((offset, _)) = src.char_indices().find(|(_, c)| !c.is_ascii()) {
        return Err(ParseError {
            offset,
            message: "only ASCII LaTeX source is supported".into(),
        });
    }
    let mut p = Parser {
        s: src.as_bytes(),
        pos: 0,
    };
    p.ws();
    let node = if p.take(b"\\begin{aligned}") {
        p.aligned()?
    } else {
        p.expr(&[])?
    };
    p.ws();
    if p.pos != p.s.len() {
        return p.fail("unexpected token after expression");
    }
    Ok(node)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}
impl<'a> Parser<'a> {
    fn fail<T>(&self, msg: impl Into<String>) -> Result<T> {
        Err(ParseError {
            offset: self.pos,
            message: msg.into(),
        })
    }
    fn starts(&self, b: &[u8]) -> bool {
        self.s[self.pos..].starts_with(b)
    }
    fn take(&mut self, b: &[u8]) -> bool {
        if self.starts(b) {
            self.pos += b.len();
            true
        } else {
            false
        }
    }
    fn command_start(&self, command: &[u8]) -> bool {
        self.starts(command)
            && !self
                .s
                .get(self.pos + command.len())
                .is_some_and(|c| c.is_ascii_alphabetic())
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }
    fn ws(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }
    fn expect(&mut self, b: &[u8], msg: &str) -> Result<()> {
        self.ws();
        if self.take(b) { Ok(()) } else { self.fail(msg) }
    }
    fn group(&mut self) -> Result<Node> {
        self.expect(b"{", "expected '{'")?;
        let n = self.expr(&[b"}".as_slice()])?;
        self.expect(b"}", "expected '}'")?;
        Ok(n)
    }
    fn text_group(&mut self) -> Result<String> {
        self.expect(b"{", "expected '{' after text command")?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return self.fail("unterminated text group"),
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') if self.starts(b"\\%") => {
                    self.pos += 2;
                    out.push('%');
                }
                Some(c) if c.is_ascii_alphanumeric() || b" +-=<>/*.,:;!?'\"[]|".contains(&c) => {
                    self.pos += 1;
                    out.push(c as char);
                }
                _ => return self.fail("unsupported character or command inside text"),
            }
        }
    }
    fn expr(&mut self, stops: &[&[u8]]) -> Result<Node> {
        let mut nodes = Vec::new();
        loop {
            self.ws();
            if self.peek().is_none()
                || stops.iter().any(|stop| {
                    if *stop == b"\\right" {
                        self.command_start(stop)
                    } else {
                        self.starts(stop)
                    }
                })
            {
                break;
            }
            if self.command_start(b"\\right")
                || self.command_start(b"\\end")
                || self.starts(b"\\\\")
                || matches!(self.peek(), Some(b'}' | b'&'))
            {
                return self.fail("unmatched delimiter or alignment token");
            }
            let before = self.pos;
            let node = self.atom()?;
            if self.pos == before {
                return self.fail("parser made no progress");
            }
            nodes.push(self.scripts(node)?);
        }
        Ok(Node::Row(nodes))
    }
    fn scripts(&mut self, base: Node) -> Result<Node> {
        let (mut sub, mut sup) = (None, None);
        loop {
            // TeX ignores whitespace between a nucleus and its scripts.
            self.ws();
            let which = match self.peek() {
                Some(b'^') => true,
                Some(b'_') => false,
                _ => break,
            };
            self.pos += 1;
            self.ws();
            let arg = if self.peek() == Some(b'{') {
                self.group()?
            } else {
                self.atom()?
            };
            let slot = if which { &mut sup } else { &mut sub };
            if slot.is_some() {
                return self.fail("duplicate superscript or subscript");
            }
            *slot = Some(Box::new(arg));
        }
        if sub.is_some() || sup.is_some() {
            Ok(Node::Script {
                base: Box::new(base),
                sub,
                sup,
            })
        } else {
            Ok(base)
        }
    }
    fn atom(&mut self) -> Result<Node> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.group(),
            Some(b'\\') => self.command(),
            Some(c) if c.is_ascii_alphanumeric() || b"+-=<>/*.,:;!?'\"()[]|".contains(&c) => {
                self.pos += 1;
                Ok(Node::Glyph((c as char).to_string()))
            }
            Some(b'%') => self.fail("write percent as \\%"),
            Some(_) => self.fail("unsupported character"),
            None => self.fail("expected an atom"),
        }
    }
    fn command(&mut self) -> Result<Node> {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if start == self.pos {
            let Some(c) = self.peek() else {
                return self.fail("incomplete command");
            };
            self.pos += 1;
            return match c {
                b'%' => Ok(Node::Glyph("\\%".into())),
                b'!' => Ok(Node::Space(-2.0)),
                b',' => Ok(Node::Space(3.0)),
                b':' => Ok(Node::Space(5.0)),
                b';' => Ok(Node::Space(8.0)),
                _ => self.fail("unsupported control symbol"),
            };
        }
        let name = std::str::from_utf8(&self.s[start..self.pos]).unwrap();
        match name {
            "frac" => {
                let a = self.group()?;
                let b = self.group()?;
                Ok(Node::Fraction(Box::new(a), Box::new(b)))
            }
            "sqrt" => {
                self.ws();
                let index = if self.take(b"[") {
                    let n = self.expr(&[b"]".as_slice()])?;
                    self.expect(b"]", "expected ']' after root index")?;
                    Some(Box::new(n))
                } else {
                    None
                };
                Ok(Node::Root(index, Box::new(self.group()?)))
            }
            "text" | "operatorname" => Ok(Node::Text(self.text_group()?)),
            "mathrm" | "unit" => self.group(),
            "mathbb" => {
                self.expect(b"{R}", "only \\mathbb{R} is supported")?;
                Ok(Node::Glyph("\\mathbb{R}".into()))
            }
            "left" => {
                let left = self.delimiter()?;
                let n = self.expr(&[b"\\right".as_slice()])?;
                self.expect(b"\\right", "expected \\right")?;
                let right = self.delimiter()?;
                if !Self::matching(&left, &right) {
                    return self.fail("mismatched delimiters");
                }
                Ok(Node::Delimited(left, Box::new(n), right))
            }
            "begin" => self.fail("aligned must be the entire request"),
            "xrightarrow" | "xleftarrow" | "xleftrightarrow" => {
                self.ws();
                if self.peek() == Some(b'[') {
                    return self.fail("optional [below] arrow labels are unsupported; use \\xrightarrow{label} with one braced label");
                }
                Ok(Node::Arrow(format!("\\{name}"), Box::new(self.group()?)))
            }
            "hat" | "widehat" | "bar" | "overline" | "tilde" | "widetilde" | "vec" | "dot"
            | "ddot" | "underline" => Ok(Node::Accent(name.into(), Box::new(self.group()?))),
            "enspace" => Ok(Node::Space(16.0)),
            "quad" => Ok(Node::Space(32.0)),
            "qquad" => Ok(Node::Space(64.0)),
            "cdots" | "ldots" => Ok(Node::Glyph(format!("\\{name}"))),
            _ if is_named(name) => Ok(Node::Text(name.into())),
            _ if is_symbol(name) => Ok(Node::Glyph(format!("\\{name}"))),
            _ => self.fail(format!("unsupported command \\{name}")),
        }
    }
    fn delimiter(&mut self) -> Result<String> {
        self.ws();
        let start = self.pos;
        if self.take(b"\\") {
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                self.pos += 1;
            }
            if start + 1 == self.pos && self.peek().is_some() {
                self.pos += 1;
            }
        } else if self.peek().is_some() {
            self.pos += 1;
        }
        let token = std::str::from_utf8(&self.s[start..self.pos]).unwrap_or("");
        if [
            "(", ")", "[", "]", "|", ".", "\\{", "\\}", "\\lbrace", "\\rbrace", "\\langle",
            "\\rangle", "\\lvert", "\\rvert",
        ]
        .contains(&token)
        {
            Ok(token.into())
        } else {
            self.fail("unsupported delimiter")
        }
    }
    fn matching(a: &str, b: &str) -> bool {
        matches!(
            (a, b),
            ("(", ")")
                | ("[", "]")
                | ("\\{", "\\}")
                | ("\\lbrace", "\\rbrace")
                | ("\\langle", "\\rangle")
                | ("|", "|")
                | ("\\lvert", "\\rvert")
        ) || a == "."
            || b == "."
    }
    fn aligned(&mut self) -> Result<Node> {
        let mut rows = Vec::new();
        let mut has_alignment_marker = false;
        let mut first_unmarked_row = None;
        loop {
            self.ws();
            let row_start = self.pos;
            let left = self.expr(&[b"&".as_slice(), b"\\\\", b"\\end{aligned}"])?;
            let right = if self.take(b"&") {
                Some(self.expr(&[b"\\\\".as_slice(), b"\\end{aligned}"])?)
            } else {
                None
            };
            if right.is_some() {
                has_alignment_marker = true;
            } else {
                first_unmarked_row.get_or_insert(row_start);
            }
            rows.push((left, right));
            self.ws();
            if self.take(b"\\end{aligned}") {
                break;
            }
            self.expect(b"\\\\", "expected row break or \\end{aligned}")?;
            if self.starts(b"\\end{aligned}") {
                return self.fail("empty trailing aligned row");
            }
        }
        if has_alignment_marker && let Some(offset) = first_unmarked_row {
            self.pos = offset;
            return self.fail("every row in an aligned block must include '&' when any row uses alignment markers; for prose in the right column, write &\\text{...}");
        }
        Ok(Node::Aligned(rows))
    }
}

fn is_named(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "sinh"
            | "cosh"
            | "tanh"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "ln"
            | "log"
            | "lg"
            | "exp"
            | "lim"
            | "limsup"
            | "liminf"
            | "max"
            | "min"
            | "sup"
            | "inf"
            | "det"
            | "dim"
            | "ker"
            | "gcd"
    )
}
fn is_symbol(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "iint"
            | "iiint"
            | "oint"
            | "sum"
            | "prod"
            | "partial"
            | "nabla"
            | "infty"
            | "ell"
            | "hbar"
            | "Re"
            | "Im"
            | "degree"
            | "circ"
            | "prime"
            | "star"
            | "alpha"
            | "beta"
            | "gamma"
            | "delta"
            | "epsilon"
            | "zeta"
            | "eta"
            | "theta"
            | "iota"
            | "kappa"
            | "lambda"
            | "mu"
            | "nu"
            | "xi"
            | "pi"
            | "rho"
            | "sigma"
            | "tau"
            | "upsilon"
            | "phi"
            | "chi"
            | "psi"
            | "omega"
            | "Gamma"
            | "Delta"
            | "Theta"
            | "Lambda"
            | "Xi"
            | "Pi"
            | "Sigma"
            | "Upsilon"
            | "Phi"
            | "Psi"
            | "Omega"
            | "pm"
            | "mp"
            | "times"
            | "cdot"
            | "div"
            | "ast"
            | "oplus"
            | "otimes"
            | "ne"
            | "equiv"
            | "approx"
            | "simeq"
            | "sim"
            | "propto"
            | "le"
            | "ge"
            | "ll"
            | "gg"
            | "in"
            | "notin"
            | "mid"
            | "parallel"
            | "to"
            | "rightarrow"
            | "longrightarrow"
            | "leftarrow"
            | "longleftarrow"
            | "leftrightarrow"
            | "Rightarrow"
            | "Longrightarrow"
            | "Leftarrow"
            | "Leftrightarrow"
            | "mapsto"
            | "uparrow"
            | "downarrow"
            | "rightleftharpoons"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn examples() {
        for s in [
            r"\text{For an ideal gas, }pV=nRT",
            r"\Delta S = \int_{T_1}^{T_2} \frac{C_p(T)}{T}\,\mathrm{d}T",
            r"\frac{\partial u}{\partial t} = \alpha \nabla^2 u",
            r"\begin{aligned}\frac{\mathrm{d}x}{\mathrm{d}t} &= v \\ \frac{\mathrm{d}v}{\mathrm{d}t} &= -\omega^2 x\end{aligned}",
            r"\mathrm{Fe^{3+}} + \mathrm{SCN^-} \rightleftharpoons \mathrm{FeSCN^{2+}}",
            r"\mathrm{CaCO_3(s)} \xrightarrow{\Delta} \mathrm{CaO(s)} + \mathrm{CO_2(g)}",
            r"x \in \mathbb{R}",
            r"\left(\frac{x}{2}\right)",
            r"\sqrt[3]{x_1} + \dot{x}",
        ] {
            parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        }
    }
    #[test]
    fn labeled_arrow_error_suggests_supported_syntax() {
        let error = parse(r"C_{10}H_{18}O\xrightarrow[\Delta]{H^+}C_{10}H_{16}+H_2O").unwrap_err();
        assert!(error.message.contains("optional [below] arrow labels"));
        assert!(error.message.contains(r"\xrightarrow{label}"));
    }
    #[test]
    fn aligned_rows_require_consistent_alignment_markers() {
        let source = r"\begin{aligned}q_{1\,atm}&=h_{fg}=2256.5\,\unit{kJ/kg}\\q_{8\,atm}&=h_{fg}\left(0.8\,\unit{MPa}\right)\approx2046\,\unit{kJ/kg}\\2256.5&>2046\\\text{More energy is required at }1\,\text{atm.}\end{aligned}";
        let error = parse(source).unwrap_err();
        assert!(
            error
                .message
                .contains("every row in an aligned block must include '&'")
        );
        assert!(source[error.offset..].starts_with(r"\text{More energy"));

        let explicitly_aligned = source.replace(r"\\\text{More energy", r"\\&\text{More energy");
        parse(&explicitly_aligned).unwrap();
        parse(r"\begin{aligned}x=1\\\text{one-column row}\end{aligned}").unwrap();
    }

    #[test]
    fn rejects_invalid() {
        for s in [
            r"\ce{H2O}",
            r"x^^2",
            r"\frac{a}",
            r"\mathbb{Q}",
            r"\left(a\right]",
            r"\begin{matrix}x\end{matrix}",
            "é",
            "x%hi",
            r"\text{hi \frac{a}{b}}",
            r"x_1_2",
        ] {
            assert!(parse(s).is_err(), "incorrectly accepted {s}");
        }
    }
}
