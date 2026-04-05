# Ternlang Decision Auditor (TernAudit)

A tool for **Ternary Observability & Resolution**. TernAudit explains the "why" behind triadic decisions, providing a clear cause chain for every -1, 0, and +1 result.

This is the governance layer for the Ternary Intelligence Stack, enabling high-stakes AI safety, legal explainability, and multi-agent conflict resolution.

## Features

- **Decision Analysis:** Ingests deliberation traces and generates human-readable audit reports.
- **Cause Chain Tracking:** Identifies veto triggers, stable attractors (equilibriums), and signal mass.
- **Conflict Resolution Engine:** Compares multiple deliberation cycles and suggests arbitration paths (e.g., Veto Overrides or Consensus Holds).
- **Agent Tracing:** Provides granular visibility into the reasoning of all 13 expert agents.

## Usage

### Analyze a Decision
```bash
cargo run -p ternlang-audit -- analyze --trace path/to/trace.json
```

### Resolve a Conflict
```bash
cargo run -p ternlang-audit -- resolve --a trace_1.json --b trace_2.json
```

## Trace Format (JSON)
The auditor expects a `DeliberationTrace` object:
```json
{
  "timestamp": "ISO-8601",
  "query": "The original request",
  "final_trit": -1 | 0 | 1,
  "confidence": 0.0 to 1.0,
  "is_stable_hold": true | false,
  "veto_triggered": true | false,
  "veto_expert": "ExpertName" | null,
  "agents": [...]
}
```

## License

LGPL-3.0-or-later (part of the Ternary Intelligence Stack)
