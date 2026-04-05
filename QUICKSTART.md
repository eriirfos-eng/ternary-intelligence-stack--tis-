# ⚡ Ternlang Quickstart Manual

Welcome to the **Ternary Intelligence Stack (TIS)**. You are now operating in balanced ternary space: **-1 (Reject), 0 (Hold), +1 (Affirm).**

This guide will take you from zero to a sovereign triadic node in 2 minutes.

---

## 1. Installation (The One-Liner)

Fire this in your terminal to install the entire stack (Compiler, VM, Translator, and Auditor):

```bash
curl -fsSL https://raw.githubusercontent.com/eriirfos-eng/ternary-intelligence-stack/main/scripts/install.sh | bash
```

---

## 2. Your First Triadic Program

Create a file named `hello.tern`:

```tern
fn main() -> trit {
    // Logic equilibrium: (+1) + (-1) = 0
    let signal: trit = consensus(1, -1);
    
    match signal {
        1  => { println("Affirmed"); }
        0  => { println("Holding in Equilibrium"); }
        -1 => { println("Rejected"); }
    }
    
    return signal;
}
```

**Run it:**
```bash
tern run hello.tern
```

---

## 3. Migration (The Translator)

Move your binary logic to triadic logic instantly.

```bash
# Convert a Python logic block to Ternlang
tern-trans migrate --input logic.py --output logic.tern
```

---

## 4. Observability (The Auditor)

Understand **why** your agent made a decision.

```bash
# Analyze a deliberation trace
tern-audit analyze --trace deliberation.json
```

---

## 5. Standard Library Explorer

TIS comes with research-grade modules ready to use:

- **`physical.robotics`**: Balance and kinematics.
- **`systems.security`**: Zero-trust triadic gates.
- **`bio.gene_regulation`**: Complex system modeling.
- **`reasoning.uncertainty_ext`**: Measuring logical tension.

Explore them in `stdlib/`.

---

## Next Steps
- 📚 [Read the Whitepaper](https://ternlang.com/whitepaper.pdf)
- 🧪 [Browse Examples](https://github.com/eriirfos-eng/ternary-intelligence-stack/tree/main/examples)
- 🏛 [Join RFI-IRFOS](https://osf.io)

**Welcome to the Post-Binary Era.**
