# Handwritten equations

These sources use the [supported LaTeX subset](../docs/latex-subset.md).
Each `.tex` file has a matching PNG in `rendered/`. PNGs are generated output
and are ignored by Git; regenerate them with your own stroke profile.

- [`stoichiometry.tex`](stoichiometry.tex) → `rendered/stoichiometry.png`:
  10 g H₂ → 4.96 mol H₂O via molar mass (2.016 g/mol H₂) and the 2:2
  H₂:H₂O mole ratio. Units are displayed without cancellation marks.
- [`integration-by-parts.tex`](integration-by-parts.tex) →
  `rendered/integration-by-parts.png`: ∫ x eˣ dx = x eˣ − ∫ eˣ dx = eˣ(x − 1) + C.
  This uses u = x, dv = eˣ dx.
- [`chemical-yields.tex`](chemical-yields.tex) → `rendered/chemical-yields.png`:
  aligned tricyclene, camphene, and isoborneol calculations followed by
  `Total: 0.32×10⁶ + 19.13×10⁶ + 2.00×10⁶ = 21.45×10⁶`.

Additional examples: `aligned.tex` (two aligned differential equations),
`calculus.tex` (partial derivative), `chemistry.tex` (hydrogen combustion),
`labeled-reaction.tex` (calcium carbonate decomposition), `prose.tex` (heat
sentence), and `thermodynamics.tex` (entropy integral). All have corresponding
PNGs with the same stem in `rendered/`.

To regenerate one with your own stroke file:

```sh
cargo run -- render "path/to/aspectwrite-strokes.json" examples/rendered/stoichiometry.png "$(cat examples/stoichiometry.tex)"
```
