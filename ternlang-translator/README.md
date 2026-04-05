# Ternlang Translator

A tool to migrate binary logic (Python, Rust, Java, C++, JavaScript) into balanced ternary **Ternlang** (`.tern`) syntax.

This tool automatically maps binary types and control flow to triadic states, emphasizing the **Right to Deliberate** by injecting `tend` (0) states for safety and exhaustiveness.

## Features

- **Automated Mapping:** Converts `bool` to `trit`, `true/false` to `affirm/reject`, and `null` to `tend`.
- **Structural Transformation:** Refactors `if/else` blocks into exhaustive ternary `match` statements.
- **Safety Injection:** Automatically adds `tend` arms to handle ambiguous or insufficient data states.
- **Detailed Insights:** Explains the triadic transformation logic for every translated file.

## Usage

### CLI

```bash
cargo run -p ternlang-translator -- --input path/to/logic.py --verbose
```

### Options

- `-i, --input <PATH>`: The source file to translate.
- `-o, --output <PATH>`: Optional output path (defaults to `<input>.tern`).
- `-v, --verbose`: Print detailed triadic logic insights.

## Translation Example

**Input (Python):**
```python
def check_safety(data):
    if data is None:
        return False
    return True
```

**Output (.tern):**
```rust
fn check_safety(data):
    match data is tend {
        return reject
    tend => { // auto-injected safety hold
    }
    reject => {
        return affirm
```

## License

LGPL-3.0-or-later (part of the Ternary Intelligence Stack)
