# Supported LaTeX subset

This document defines version **0.1** of the input language for aspectwrite.
It is a closed, renderer-oriented subset of LaTeX, not a TeX interpreter. The
parser must reject an unknown command or an out-of-subset construct with a
location, rather than passing it through or trying to expand it.

The subset is deliberately small enough that an LLM can reliably generate it
and that every visible, fixed-size glyph can have a handwriting sample.

## Input model

* A request contains one UTF-8 source string. The first implementation accepts
  ASCII source only; Greek and other mathematical symbols are written with the
  commands listed below.
* There is no preamble, document environment, package loading, macro
  definition, comment syntax, or math-mode switch. The complete request is
  already in the supported math/display mode.
* Whitespace outside `\text{...}` is insignificant. Whitespace in a text block
  is preserved.
* Braces used for arguments and grouping are syntax and are not drawn. To draw
  a brace, use `\left\{` and `\right\}`.
* Letters next to one another in math mode are separate implicit-multiplication
  atoms. Use `\text{...}` for words and prose.
* A number is a run of digits with an optional decimal part, for example
  `12`, `0.5`, and `101.3`. Scientific notation should be written with
  `\times`, for example `1.2\times10^{-3}`.

## Grammar

The following is an intentionally simplified EBNF. The command tables below
are part of the grammar: a command not in those tables is invalid.

```text
document       ::= expression | aligned
expression     ::= item*
item           ::= scripted_atom | binary | relation | punctuation | spacing
scripted_atom ::= nucleus script*
nucleus        ::= literal
                 | command_symbol
                 | named_operator
                 | group
                 | fraction
                 | radical
                 | text_atom
                 | roman_atom
                 | unit_atom
                 | accent
                 | number_set
                 | labeled_arrow
                 | delimited
number_set     ::= "\mathbb{R}"
labeled_arrow  ::= ("\xrightarrow" | "\xleftarrow" | "\xleftrightarrow") group
script         ::= "^" script_argument | "_" script_argument
script_argument ::= group | simple_nucleus
simple_nucleus ::= literal | command_symbol | named_operator | group
                 | fraction | radical | text_atom | roman_atom
                 | unit_atom | accent

group          ::= "{" expression "}"
fraction       ::= "\frac" group group
radical        ::= "\sqrt" ("[" expression "]")? group
text_atom      ::= "\text" text_group
roman_atom     ::= ("\mathrm" | "\operatorname") group
unit_atom      ::= "\unit" group
accent         ::= accent_command group

delimited      ::= "\left" delimiter expression "\right" delimiter
aligned        ::= "\begin{aligned}" row ("\\" row)*
                  "\end{aligned}"
row            ::= expression | expression "&" expression
text_group     ::= "{" text_character* "}"
```

`script*` means that a nucleus may have one superscript and one subscript, in
any order (for example, `x_i^2` and `x^2_i`). A second subscript or second
superscript on the same nucleus is invalid. A grouped script can contain a
complete expression, so `x_{i+1}`, `\mathrm{SO_4^{2-}}`, and
`\int_{T_1}^{T_2}` are valid.

An unbraced script is exactly one simple nucleus: `x^2`, `x^\prime`, and
`x^\mathrm{T}` are valid, but `x^2+1` means `(x^2)+1`, not `x^(2+1)`.

`aligned` is the only environment. It provides multiple rows for equations;
`&` is an invisible alignment point and is allowed at most once per row. An
`aligned` block may omit `&` on every row, in which case the rows form one
right-aligned column. If any row uses `&`, every row must use it: the first
column is right-aligned and the second starts to its right. For prose in the
second column, start the row with `&`, for example `&\text{A note}`. A mixed
block is rejected instead of guessing where an unmarked row belongs. A
renderer may also accept a single-row `aligned`, but it must have the same
meaning as its contents.

The delimiters in a `\left ... \right` pair must match. A `.` delimiter is
invisible and is useful for one-sided delimiters, such as `\left. ...
\right|`.

## Literal characters

The following characters may be written literally in an expression:

* `A-Z`, `a-z`, and `0-9`;
* `+ - = < > / *`;
* `. , : ; ! ? ' "`;
* the delimiter characters `(`, `)`, `[`, `]`, and `|`.

`_`, `^`, `{`, `}`, `&`, and `\` are syntax and cannot be used as literal
characters. A percent sign is written as `\%`; an unescaped percent sign is
not a comment and is rejected. Other punctuation and all raw non-ASCII
characters are rejected in version 0.1.

The control symbol `\%` is also accepted wherever a literal percent sign is
needed. Inside `\text{...}`, letters, digits, spaces, and the punctuation
above are allowed. A text block cannot contain a nested command or an
unescaped brace.
This is enough for prose such as `\text{The pressure is constant.}` and for
mixed content such as `\text{For an ideal gas, }pV=nRT`.

## Structural and styling commands

| Command | Meaning and restrictions |
| --- | --- |
| `\frac{numerator}{denominator}` | Fraction with two required braced expressions. |
| `\sqrt{radicand}` | Square root. `\sqrt[index]{radicand}` is also accepted; the optional index is syntax in square brackets, not a drawn bracket. |
| `\text{text}` | Upright prose run. TeX commands and nested math are not allowed inside. |
| `\mathrm{expression}` | Upright expression, useful for units, chemical formulas, and differential `d`. |
| `\operatorname{name}` | Upright named operator made from text characters. |
| `\unit{expression}` | Aspectwrite convenience command. It has the same layout as `\mathrm`, but identifies the contents as a unit for validation and future spacing rules. Unit expressions may contain letters, Greek symbols, numbers, scripts, and spacing commands. |
| `\mathbb{R}` | The real-number set symbol. This is the only blackboard-bold command in version 0.1. |
| `\left...\right...` | A matched delimiter pair which scales to the enclosed expression. See [delimiters](#delimiters). |
| `\begin{aligned}...\end{aligned}` | Multiple rows, separated by `\\`. If any row uses `&`, every row must use one; no other environment is accepted. |
| `\xrightarrow{label}`, `\xleftarrow{label}`, `\xleftrightarrow{label}` | A reaction arrow with one required label above it. Optional arguments are not supported. |

The supported accents are `\hat`, `\widehat`, `\bar`, `\overline`,
`\tilde`, `\widetilde`, `\vec`, `\dot`, `\ddot`, and `\underline`.
Each takes exactly one braced expression, for example `\dot{x}`,
`\ddot{q}`, and `\overline{AB}`. The accent stroke is a layout primitive;
it is not a separate character that has to be collected.

## Spacing commands

Only these explicit spacing commands are accepted:

* `\!` — negative thin space;
* `\,` — thin space;
* `\:` — medium space;
* `\;` — thick space;
* `\quad` and `\qquad` — large and extra-large spaces.

Ordinary source whitespace is ignored in math mode. In particular, unit
spacing should be written explicitly, for example
`\unit{kg\,m^2\,s^{-2}}`.

## Named operators and calculus symbols

The following commands produce upright named operators:

```text
\sin  \cos  \tan  \cot  \sec  \csc
\sinh \cosh \tanh
\arcsin \arccos \arctan
\ln   \log  \lg   \exp
\lim  \limsup \liminf \max \min \sup \inf
\det  \dim  \ker  \gcd
```

These named operators are composed from the collected upright Latin letters;
they do not require separate handwriting samples. The large operators are:

```text
\int  \iint  \iiint  \oint  \sum  \prod
```

`\iint` and `\iiint` are repeated forms of `\int`; the renderer composes
them from the one integral sample. Their scripts are valid in the normal way.
In display layout, limits on a large operator are placed above and below it;
ordinary atom scripts stay to the upper-right and lower-right. Thus both
`\int_0^1` and
`\sum_{i=1}^n` have unambiguous layout.

The following calculus and physics symbols are supported:

```text
\partial  \nabla  \infty  \ell  \hbar  \Re  \Im
\degree   \circ   \prime  \star
\cdots    \ldots
```

`\degree` is the degree sign used in temperatures and angles. `\partial`
and `\nabla` are distinct handwritten glyphs. A differential is normally
written upright, for example `\mathrm{d}x` or `\mathrm{d}T`. The prime mark
shares the literal apostrophe glyph. `\cdots` and `\ldots` are composed from
the period glyph, and therefore do not need separate handwriting samples.

## Greek letters

Lowercase commands:

```text
\alpha \beta \gamma \delta \epsilon \zeta \eta \theta
\iota \kappa \lambda \mu \nu \xi \pi \rho \sigma \tau
\upsilon \phi \chi \psi \omega
```

Uppercase commands:

```text
\Gamma \Delta \Theta \Lambda \Xi \Pi \Sigma
\Upsilon \Phi \Psi \Omega
```

The variant commands (`\varepsilon`, `\vartheta`, `\varkappa`, `\varpi`,
`\varrho`, `\varsigma`, and `\varphi`) are deliberately deferred. The first
profile uses one canonical shape for each common Greek letter, which keeps the
handwriting inventory finite and unambiguous.

## Binary operators, relations, and arrows

Binary operators:

```text
\pm  \mp  \times  \cdot  \div  \ast  \oplus  \otimes
```

`\ast` uses the same visual sample as the literal `*`.

Relations:

```text
\ne  \equiv  \approx  \simeq  \sim  \propto
\le  \ge  \ll  \gg  \in  \notin  \mid  \parallel
```

Set-membership symbols are included so expressions such as `x\in\mathbb{R}`
are possible. Other set-operation symbols are deferred until they are needed
by a supported use case.

Arrows and implication symbols:

```text
\to  \rightarrow  \longrightarrow
\leftarrow  \longleftarrow  \leftrightarrow
\Rightarrow  \Longrightarrow  \Leftarrow  \Leftrightarrow
\mapsto  \uparrow  \downarrow  \rightleftharpoons
```

Labeled reaction arrows are `\xrightarrow{label}`, `\xleftarrow{label}`, and
`\xleftrightarrow{label}`. Each takes one required braced label above the
arrow; optional arguments are not supported.

The short `\to` and `\rightarrow` forms are equivalent. Arrow shafts may be
made longer by the layout engine when needed; the arrowhead is still one
supported symbol.

General logical quantifiers and other set-operation symbols are intentionally
outside the first profile.

## Delimiters

Literal `(`, `)`, `[`, `]`, and `|` are accepted for short expressions. Use
`\left` and `\right` when the contents include a fraction, root, or
large operator:

```latex
\left(\frac{a}{b}\right)
\left[\int_0^1 f(x)\,\mathrm{d}x\right]
```

The following delimiter tokens are accepted after `\left` or `\right`:

```text
(  )  [  ]  |  \{  \}  \lbrace  \rbrace
\langle  \rangle  \lvert  \rvert  .
```

Delimiters are intentionally **not** part of the fixed glyph set collected
from the user. The collector will not ask for parentheses, brackets, braces,
or angle delimiters. The renderer will draw them with a small parametric
stroke using the current handwritten width and line height, so they can scale
around a fraction or root. This is the one planned procedural fallback; all
other visible fixed-size symbols come from the stroke file.

## Intended examples

All of the following are in the subset and are acceptance examples for the
parser and renderer.

### Physics and thermodynamics

```latex
\text{For an ideal gas, }pV=nRT
```

```latex
\Delta U = Q - W
```

```latex
\Delta S = \int_{T_1}^{T_2} \frac{C_p(T)}{T}\,\mathrm{d}T
```

```latex
P = 101.3\,\unit{kPa},\quad T = 25\,\degree\unit{C}
```

```latex
\eta = 1 - \frac{T_c}{T_h}
```

### Calculus and differential equations

```latex
\frac{\mathrm{d}y}{\mathrm{d}x} = ky
```

```latex
\frac{\partial u}{\partial t} = \alpha \nabla^2 u
```

```latex
\int_0^\infty e^{-x}\,\mathrm{d}x = 1
```

```latex
\begin{aligned}
\frac{\mathrm{d}x}{\mathrm{d}t} &= v \\
\frac{\mathrm{d}v}{\mathrm{d}t} &= -\omega^2 x
\end{aligned}
```

### Chemical equations

```latex
2\mathrm{H_2} + \mathrm{O_2} \rightarrow 2\mathrm{H_2O}
```

```latex
\mathrm{CH_4} + 2\mathrm{O_2} \rightarrow \mathrm{CO_2} + 2\mathrm{H_2O}
```

```latex
\mathrm{Fe^{3+}} + \mathrm{SCN^-} \rightleftharpoons \mathrm{FeSCN^{2+}}
```

```latex
\mathrm{CaCO_3(s)} \xrightarrow{\Delta} \mathrm{CaO(s)} + \mathrm{CO_2(g)}
```

The last example uses `\xrightarrow`, which is included as the one labeled
reaction-arrow command. Its required braced label is placed above the arrow.
The accepted labeled-arrow commands are `\xrightarrow{label}`,
`\xleftarrow{label}`, and `\xleftrightarrow{label}`. They may not have an
optional argument or a second label.

### Number-set notation

```latex
x \in \mathbb{R}
```

### Prose mixed with math

```latex
\text{The heat added is }Q = mc\Delta T\text{.}
```

```latex
\text{At constant pressure, }\Delta H = nC_p\Delta T\text{.}
```

## Explicit non-goals for version 0.1

The following are rejected so that the language remains deterministic:

* arbitrary TeX control sequences, macro definitions, package commands,
  comments, preambles, `$...$`, `\(...\)`, and `\[...\]`;
* `\left`/`\right` variants other than the delimiter tokens listed above;
* matrices, arrays, cases, fractions made with `\over`, and alignment outside
  `aligned`;
* `\dfrac`, `\tfrac`, `\binom`, `\color`, `\style`, `\font`, and
  user-defined commands;
* arbitrary HTML, Markdown, or Unicode glyphs in `\text`;
* automatic chemical parsing commands such as `\ce` and package-specific
  unit commands such as `\SI`;
* stretchy accents or manually requested sizes other than the supported
  accents and `\left`/`\right` delimiters.

If a future phase needs one of these, it should be added to this document and
to the fixed-glyph/procedural-glyph classification before parser code is
changed.

## Phase-two glyph contract

The handwriting collector asks for at least one sample of each canonical visual
glyph; letters and digits may additionally have a second and third variant:

1. `A-Z`, `a-z`, and `0-9`;
2. the common Greek commands in this document;
3. the fixed calculus, physics, operator, relation, and arrow glyphs in the
   tables, plus `\Re`, `\Im`, and `\mathbb{R}`;
4. the literal punctuation used by `\text` and expressions, except the
   scalable delimiter characters listed below.

Named operators such as `\sin` and `\lim` are composed from the collected
Latin letters and do not get separate entries. Equivalent commands share a
sample: `\to`, `\longrightarrow`, and `\xrightarrow` use the canonical
right-arrow sample; `\iint` and `\iiint` repeat the canonical integral; and
`\ast` and `\prime` reuse `*` and the apostrophe respectively. These aliases
are recorded in the stroke file instead of creating duplicate prompts.

The collector should not ask for syntax-only tokens (`{}`, `^`, `_`, `&`,
command names), layout primitives (fraction bars, radical overbars, accent
strokes, repeated dots, and arrow shafts), or scalable delimiters. The exact
canonical sample keys and their aliases are emitted in the stroke file.
