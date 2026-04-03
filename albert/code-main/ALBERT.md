# ALBERT.md

This file provides guidance to the **Albert-Code** agent loop when working with code in this repository.

## Detected stack
- Languages: Python, Rust.
- Frameworks: Ollama (albert:latest).

## Verification
- **Albert Chat Loop**: Run `python3 -m src.main chat` to enter the interactive terminal session.
- **Direct Tool Execution**: `python3 -m src.main exec-tool <tool_name> <payload>`
- **Rust verification** (from `rust/`): `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
- `src/` and `tests/` are both present; update both surfaces together when behavior changes.

## Repository shape
- `src/`: Functional Python implementation of the **Albert-Code** CLI and tool harness.
- `rust/`: Rust workspace for high-performance runtime research.
- `tests/`: Validation surfaces.

## Working agreement
- **Sovereignty First**: Ensure Albert acts via tools rather than narrating.
- **Terminal Speed**: Prioritize terminal-friendly output and fast inference.
- **Functional Tools**: Keep `src/tools.py` aligned with the sovereign capabilities of RFI-IRFOS.
- **Session Management**: Ensure sessions are persisted to `.port_sessions/` (JSON) and memory is logged to `albert_os.db` (SQLite).

## Tool dependencies
- `ollama`: For local LLM inference.
- `psutil`: For system telemetry.
- `duckduckgo-search`: For web research.
- `requests`: For URL fetching.
- `pyautogui`: For screen capture.
- `scikit-learn`: For library search (TF-IDF).
