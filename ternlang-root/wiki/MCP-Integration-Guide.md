# MCP Integration Guide

Connect any AI agent to the Ternary Intelligence Stack via the Model Context Protocol.

---

## Setup

Build the MCP server binary:

```bash
cd "Ternary Intelligence Stack (TIS)/ternlang-root"
cargo build --release -p ternlang-mcp
# binary: target/release/ternlang-mcp
```

Add to your MCP client config:

```json
{
  "mcpServers": {
    "ternlang": {
      "command": "/absolute/path/to/ternlang-mcp",
      "args": []
    }
  }
}
```

For Claude Desktop: add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows).

---

## Tool Reference

### `trit_decide`
**The core reasoning tool.** Pass in any numeric evidence signals on [−1.0, +1.0]. Returns a scalar temperature, zone classification, and confidence.

```json
// Input
{
  "evidence": [0.7, -0.2, 0.5, 0.1],
  "min_confidence": 0.4
}

// Output
{
  "scalar": 0.275,
  "trit": 0,
  "label": "tend",
  "confidence": 0.175,
  "is_actionable": false,
  "tend_boundary": 0.3333,
  "recommendation": "Tend — scalar 0.275 is within the deliberation zone. Gather more evidence before acting.",
  "per_signal": [
    {"index": 0, "raw": 0.7,  "label": "affirm", "confidence": 0.55, "trit": 1},
    {"index": 1, "raw": -0.2, "label": "tend",   "confidence": 0.4,  "trit": 0},
    {"index": 2, "raw": 0.5,  "label": "affirm", "confidence": 0.25, "trit": 1},
    {"index": 3, "raw": 0.1,  "label": "tend",   "confidence": 0.7,  "trit": 0}
  ]
}
```

---

### `trit_vector`
**Multi-source reasoning with named dimensions.** The full agent deliberation tool.

```json
// Input
{
  "dimensions": [
    {"label": "user_sentiment",    "value":  0.75, "weight": 1.5},
    {"label": "safety_check",      "value": -0.60, "weight": 3.0},
    {"label": "relevance_score",   "value":  0.85, "weight": 1.0},
    {"label": "context_alignment", "value":  0.20, "weight": 1.0}
  ],
  "min_confidence": 0.6
}

// Output
{
  "aggregate": {
    "scalar": -0.09,
    "trit": 0,
    "label": "tend",
    "confidence": 0.73,
    "is_actionable": false
  },
  "dominant": {
    "label": "safety_check",
    "zone": "reject",
    "confidence": 0.405
  },
  "recommendation": "Tend — aggregate scalar -0.090. Strongest signal: safety_check.",
  "breakdown": [
    {"label": "user_sentiment",    "raw": 0.75,  "weight": 1.5, "trit": 1,  "zone": "affirm", "confidence": 0.625},
    {"label": "safety_check",      "raw": -0.6,  "weight": 3.0, "trit": -1, "zone": "reject", "confidence": 0.405},
    {"label": "relevance_score",   "raw": 0.85,  "weight": 1.0, "trit": 1,  "zone": "affirm", "confidence": 0.775},
    {"label": "context_alignment", "raw": 0.20,  "weight": 1.0, "trit": 0,  "zone": "tend",   "confidence": 0.4}
  ]
}
```

**Note:** safety_check at weight 3.0 pulls the aggregate into the tend zone despite positive sentiment and relevance. The agent correctly holds — it shouldn't act when a high-weight safety signal is in the reject zone.

---

### `trit_consensus`
Merge two independent ternary judgements.

```json
{"a": 1, "b": -1}   → {"result": 0, "label": "tend", "carry": 0}
{"a": 1, "b":  1}   → {"result": 1, "label": "affirm", "carry": 0}
{"a": -1, "b": -1}  → {"result": -1, "label": "reject", "carry": 0}
```

---

### `trit_eval`
Evaluate a ternlang expression on the BET VM.

```json
{"expression": "consensus(1, -1)"}
{"expression": "let x: trit = 1; return invert(x);"}
```

---

### `ternlang_run`
Compile and run a full `.tern` program.

```json
{
  "code": "fn main() { let x: trit = 1; let y: trit = -1; return consensus(x, y); }"
}
```

---

### `quantize_weights`
Convert float model weights to balanced ternary.

```json
{
  "weights": [0.9, 0.1, -0.8, 0.05, -0.92, 0.3],
  "threshold": 0.4
}
// → {"trits": [1, 0, -1, 0, -1, 0], "sparsity": 0.5, "nnz": 3}
```

---

### `sparse_benchmark`
Demonstrate TSPARSE_MATMUL efficiency.

```json
{"rows": 8, "cols": 8}
// → ops_reduction_factor, weight_sparsity, skip_rate, summary string
```

---

## Agent Integration Pattern

The recommended agent decision loop using ternary:

```
1. Collect evidence signals from all sources → values ∈ [−1.0, +1.0]
2. Call trit_vector with named dimensions + weights
3. Check aggregate.is_actionable
   - false (tend) → gather more evidence, query additional sources
   - true (affirm) → act with confidence aggregate.confidence
   - true (reject) → decline/refuse with confidence aggregate.confidence
4. Log the breakdown for explainability
```

This pattern replaces binary threshold heuristics ("`if confidence > 0.7: act`") with a principled deliberation loop that never forces a decision before the evidence warrants it.
