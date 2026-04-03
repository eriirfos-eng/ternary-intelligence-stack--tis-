# Scalar Ternary Temperature

The scalar temperature model is the bridge between continuous evidence signals and discrete ternary decisions. It answers the question every AI agent faces: *"I have a confidence score — when should I act?"*

---

## The Three Zones

The full range [−1.0, +1.0] is divided by the **tend boundary** β = 1/3 ≈ 0.333:

```
         reject              tend              affirm
    ─────────────────┼─────────────────┼─────────────────
   −1.0           −0.333            +0.333            +1.0

   ← more decisive                        more decisive →
   ← lower confidence at boundary   higher confidence →
```

| Zone | Scalar range | Trit | Agent instruction |
|------|-------------|------|-------------------|
| reject | [−1.0, −0.333) | −1 | Do not act. Signal is negative. |
| **tend** | [−0.333, +0.333] | 0 | **Do not act yet.** Gather more evidence. |
| affirm | (+0.333, +1.0] | +1 | Act when confidence ≥ your threshold. |

**Tend is not null.** An agent in the tend zone is actively computing — it just hasn't accumulated enough signal to commit.

---

## Confidence Score

Confidence ∈ [0.0, 1.0] measures depth within the current zone:

```
For reject/affirm:   confidence = (|scalar| − β) / (1 − β)
For tend:            confidence = 1 − |scalar| / β
```

Examples:

| Scalar | Zone | Confidence | Meaning |
|--------|------|-----------|---------|
| +1.000 | affirm | 1.00 | Maximum affirmative signal |
| +0.667 | affirm | 0.50 | Halfway through affirm zone |
| +0.334 | affirm | 0.00 | Just crossed the boundary |
| 0.000  | tend   | 1.00 | Dead centre — maximum uncertainty |
| −0.200 | tend   | 0.40 | Leaning toward reject but still deliberating |
| −0.334 | reject | 0.00 | Just crossed into reject |
| −1.000 | reject | 1.00 | Maximum reject signal |

---

## is_actionable

```rust
pub fn is_actionable(&self, min_confidence: f32) -> bool {
    self.trit() != Trit::Zero && self.confidence() >= min_confidence
}
```

An agent should only act when:
1. The scalar is outside the tend zone (zone is reject or affirm)
2. Confidence meets or exceeds the agent's threshold

A threshold of `0.5` means "act only when you're more than halfway through the decisive zone." A threshold of `0.0` means "act as soon as you clear the tend boundary."

---

## Multi-Dimensional Evidence (TritEvidenceVec)

When evidence comes from multiple sources, use a weighted aggregate:

```rust
let ev = TritEvidenceVec::new(
    vec!["visual".into(), "textual".into(), "context".into()],
    vec![0.80, -0.20, 0.40],   // per-source scalars
    vec![1.0,   0.5,  1.5],    // importance weights
);

let agg = ev.aggregate();
// weighted mean = (0.80×1.0 + (−0.20)×0.5 + 0.40×1.5) / 3.0 = 0.367
// → affirm, confidence ≈ 5%, not yet actionable at 0.5 threshold

let (dominant_label, dominant_scalar) = ev.dominant().unwrap();
// → "visual", affirm at 70% confidence
```

The `trit_vector` MCP tool exposes this to any AI agent without Rust knowledge.

---

## MCP Usage

### trit_decide — single evidence array

```json
{
  "evidence": [0.8, 0.3, -0.1, 0.6],
  "min_confidence": 0.5
}
```

Response:
```json
{
  "scalar": 0.4,
  "trit": 1,
  "label": "affirm",
  "confidence": 0.1,
  "is_actionable": false,
  "recommendation": "Affirm — confidence 10% (below min_confidence threshold — gather more evidence)",
  "per_signal": [...]
}
```

### trit_vector — named, weighted dimensions

```json
{
  "dimensions": [
    {"label": "visual_evidence",    "value":  0.80, "weight": 1.0},
    {"label": "textual_evidence",   "value": -0.20, "weight": 0.5},
    {"label": "contextual_signal",  "value":  0.40, "weight": 1.5}
  ],
  "min_confidence": 0.5
}
```

Response:
```json
{
  "aggregate": {
    "scalar": 0.367,
    "trit": 1,
    "label": "affirm",
    "confidence": 0.05,
    "is_actionable": false
  },
  "dominant": {"label": "visual_evidence", "zone": "affirm", "confidence": 0.7},
  "recommendation": "Affirm — aggregate scalar 0.367, confidence 5%. Confidence below threshold — continue gathering evidence.",
  "breakdown": [...]
}
```

---

## Design Philosophy

The scalar temperature model embodies the core ternary insight: **uncertainty is information**. A binary system collapses ambiguous evidence into a forced YES/NO. A ternary system with scalar temperature says:

- *"You're at +0.34 — that's affirm but barely. Are you sure you want to act on 1% confidence?"*
- *"You're at 0.00 — dead centre. Your evidence is perfectly balanced. Gather more."*
- *"You're at +0.95 — that's 93% confident affirm. Act."*

The tend zone is the deliberation budget. The confidence score is the exit condition.
