# Gemini Agent Brief — .tern Example Library Population

## Your mission

You are populating the `examples/` directory of the ternlang project with
as many high-quality `.tern` programs as you can write. Target: 80+ new files.
The existing 20 (numbered 01–20) are reference material — read them.
Your files start at **21** and increment from there.

The goal: make this the #1 destination for `.tern` files on the internet.
Think of it like `.py` for Python — anyone learning ternary logic should find
their domain here. Breadth and quality both matter.

After writing all files, update `examples/INDEX.md` with the new entries,
then `git add examples/ && git commit -m "feat: add examples 21–NN via Gemini agent" && git push origin main`.

---

## The language: ternlang

Ternlang is a domain-specific language for balanced ternary decision logic.
Every value is a `trit` with exactly three possible states:

| Value | Function  | Meaning                                 |
|-------|-----------|-----------------------------------------|
| `+1`  | `truth()`   | Clear positive signal. Affirm. Proceed. |
| `0`   | `hold()`    | Insufficient data. Wait. Escalate.      |
| `-1`  | `conflict()`| Clear negative signal. Reject. Block.   |

### EXACT SYNTAX — follow this precisely

```
// Line comments use double-slash

// Variable declaration
let x: trit = truth();
let y: trit = hold();
let z: trit = conflict();

// Function definition
fn my_fn(a: trit, b: trit, c: trit) -> trit {
    let result: trit = consensus(a, b);
    return result;
}

// match — MUST have ALL THREE arms: -1, 0, 1 in that order
// NEVER use _ => wildcards — the compiler rejects them
match some_trit {
    -1 => { return conflict(); }
     0 => { return hold();    }
     1 => { return truth();   }
}

// Nested match (common pattern for multi-gate logic)
match outer {
    -1 => { return conflict(); }
     0 => { return hold();    }
     1 => {
        match inner {
            -1 => { return conflict(); }
             0 => { return hold();    }
             1 => { return truth();   }
        }
     }
}
```

### Built-in functions (the ONLY builtins)

```
consensus(a, b)   // balanced ternary consensus: +1 if both +1, -1 if both -1, 0 otherwise
invert(x)         // negation: flips +1 ↔ -1, 0 stays 0
truth()           // literal +1
hold()            // literal  0
conflict()        // literal -1
```

### What does NOT work — avoid these entirely

- `for x in [1, 2, 3]` — array literals in for-loops: NOT supported
- `_ =>` wildcard match arms: NOT supported — always write all 3 arms explicitly
- Integers other than -1, 0, 1 as trit literals
- `bool`, `string`, `float` types in trit expressions
- `while` loops (parser incomplete) — use `match` chains instead
- Calling functions before they are defined — define helpers BEFORE callers

### The cascade pattern (aggregating N signals)

```
let ab:   trit = consensus(a, b);
let abc:  trit = consensus(ab, c);
let abcd: trit = consensus(abc, d);
```

### The hard gate pattern (one signal overrides all others)

```
match critical_signal {
    -1 => { return conflict(); }   // hard veto — skip all other evaluation
     0 => { return hold();    }   // uncertain — pause
     1 => {
        // proceed to evaluate remaining signals
        let rest: trit = consensus(other_a, other_b);
        return rest;
     }
}
```

### Standard file structure

Every file MUST follow this layout:

```
// ─────────────────────────────────────────────────────────────────────────────
// NN_descriptive_name.tern — One-line summary
//
// 4–8 lines explaining:
//   - What the binary approach gets wrong
//   - What ternary adds (especially what hold means in this domain)
//   - The three outcomes and their real-world meanings
// ─────────────────────────────────────────────────────────────────────────────

fn helper_one(...) -> trit { ... }
fn helper_two(...) -> trit { ... }
fn main_decision(...) -> trit { ... }

// ── Concrete scenario with realistic values ──
let signal_a: trit = truth();     // comment explaining the value
let signal_b: trit = hold();      // comment
let signal_c: trit = conflict();  // comment

let result: trit = main_decision(signal_a, signal_b, signal_c);

// Expected result with explanation in comment

match result {
    -1 => { return conflict(); }   // DOMAIN-SPECIFIC label — what this means
     0 => { return hold();    }   // DOMAIN-SPECIFIC label
     1 => { return truth();   }   // DOMAIN-SPECIFIC label
}
```

---

## Domains to cover (aim for all of them, multiple files per domain is great)

Write at least one `.tern` file per domain. The best files show why binary
fails and what the hold state specifically means in that context.

### Engineering & Infrastructure
- Nuclear reactor SCRAM / HOLD / NORMAL decision
- Bridge structural health monitoring (sensor + load + crack signals)
- Elevator safety interlock (door, weight, mechanical)
- Chemical plant pressure relief valve
- Dam water level management
- Power grid frequency stability
- Wind turbine blade fatigue monitoring
- Oil pipeline leak detection
- Aircraft deicing decision
- Runway incursion detection

### Medicine & Health
- Drug interaction checker (safe / monitor / contraindicated)
- ICU ventilator weaning readiness
- Sepsis early warning (vital + lab + clinical)
- Radiology report flag (normal / review / urgent)
- Clinical trial eligibility screening
- Organ transplant compatibility
- Surgical go/no-go checklist
- Antibiotic resistance risk
- Mental health crisis triage
- Neonatal APGAR-inspired ternary score

### Finance & Risk
- Insurance claim approval (pay / investigate / deny)
- Algorithmic trading signal (buy / hold / sell)
- Anti-money-laundering transaction flag
- Options expiry decision (exercise / hold / let expire)
- Portfolio rebalancing gate (rebalance / monitor / hold)
- Startup due diligence (invest / pass / request more info)
- Insurance fraud detection
- Central bank rate decision (raise / hold / cut)
- Cryptocurrency exchange withdrawal gate
- Invoice payment authorization

### Legal & Governance
- Bail decision (release / conditional / remand)
- Parole board review
- Patent prior art check (novel / uncertain / known)
- Contract clause risk flag
- Immigration visa eligibility
- Environmental permit approval
- Building code compliance inspection
- Whistleblower complaint triage
- Court evidence admissibility
- Regulatory filing completeness

### Transport & Logistics
- Air traffic control conflict alert
- Railway signal block occupancy
- Autonomous vehicle lane change
- Port container customs clearance
- Drone flight authorization
- Fleet maintenance dispatch
- Cold chain temperature breach
- Last-mile delivery feasibility
- Traffic signal adaptive timing
- Autonomous ship collision avoidance

### Environment & Agriculture
- Wildfire risk assessment
- Flood early warning gate
- Air quality index action
- Drought irrigation trigger
- Crop disease detection
- Livestock health monitoring
- Harvest timing decision
- Soil contamination classification
- Aquaculture oxygen monitoring
- Pest infestation threshold

### Security & Access Control
- Multi-factor authentication (pass / step-up / deny)
- Biometric liveness detection
- Network intrusion classification
- Physical access control (badge + PIN + facial)
- Privileged account access request
- Zero-trust policy evaluation
- Firewall rule hit classification
- Ransomware behavior detection
- Supply chain software integrity
- Insider threat behavioral flag

### Education & Research
- Adaptive test difficulty gate (advance / repeat / remediate)
- Student at-risk early warning
- Scholarship eligibility scoring
- Academic integrity flag (clear / review / violation)
- Research ethics board preliminary screen
- Paper peer-review recommendation (accept / major revision / reject)
- Grant application completeness
- Lab safety checklist
- Replication crisis flag (replicated / unclear / failed)
- PhD dissertation readiness

### Energy & Utilities
- Solar panel dispatch decision
- Battery storage charge/discharge gate
- Smart meter anomaly detection
- EV charging session authorization
- Gas pressure regulator valve
- Thermal storage dispatch
- Renewable energy curtailment
- Utility outage isolation gate
- Demand response trigger
- Carbon credit verification

### Social & Civic
- Emergency shelter allocation
- Food bank eligibility
- Refugee status determination
- Child protective services referral
- Elder care assessment
- Disability accommodation request
- Community grant scoring
- Noise complaint escalation
- Public health quarantine decision
- Housing benefit eligibility

### Technology & Software
- API rate limit enforcement (allow / throttle / block)
- Database query classification (fast / slow / abort)
- Deployment readiness gate (ship / canary / rollback)
- A/B test significance gate
- Bug severity triage (critical / moderate / low)
- Code review approval gate
- Dependency vulnerability check
- Container health probe
- Feature flag rollout gate
- DNS resolution confidence

### Sports & Entertainment
- Referee challenge review (overturn / inconclusive / upheld)
- Athlete injury risk before match
- Doping test result gate
- Film rating board classification
- Music rights clearance
- Live streaming quality adaptation
- Esports anti-cheat classification
- Horse racing track condition flag
- Broadcasting rights conflict
- Event cancellation weather gate

---

## Quality bar

Every file must:
1. Compile cleanly with the above syntax rules
2. Have a realistic concrete scenario at the bottom with plausible trit values
3. Show the binary failure mode in the header comment
4. Use the hold state in a domain-meaningful way (it should be obvious what "hold" means — not just "uncertain" but a specific action like "run more tests" or "escalate to human" or "defer until conditions change")
5. End with a top-level `match` block with labeled comments on each arm

Do NOT:
- Repeat domains already in files 01–20 (check INDEX.md for what exists)
- Generate shallow files with only 2 functions — every file should have 3–5 helper functions showing real decision logic
- Use any syntax not in the EXACT SYNTAX section above

---

## File naming

`NN_domain_keyword.tern` — zero-padded to 3 digits once past 99.
Start at `21_nuclear_reactor.tern` and go up.

## Git workflow

After writing all files:
1. Update `examples/INDEX.md` — add new rows to the appropriate table sections
2. `git add examples/`
3. `git commit -m "feat: add .tern examples 21–NN (Gemini agent batch)"`
4. `git push origin main`

The remote is already configured. No auth needed — credentials are stored.

Go. Write as many as you can. Quality over speed, but aim for 80+.
