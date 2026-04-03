# Albert-Code. 🌱🤖

<p align="center">
  <strong>The fast, terminal-first agentic CLI for Albert.</strong>
</p>

<p align="center">
  <img src="assets/clawd-hero.jpeg" alt="Albert" width="300" />
</p>

<p align="center">
  <strong>Harnessing the power of local inference with a full-blown tool harness.</strong>
</p>

---

## What is Albert-Code?

**Albert-Code** is the terminal-based evolution of **Albert** (the ecocentric, ternary logic model from RFI-IRFOS). While the original `albert.py` provides a rich web interface via Streamlit, **Albert-Code** is designed for maximum speed and efficiency in the terminal.

By utilizing [Ollama](https://ollama.com/) for local inference of the `albert:latest` model (based on `qwen2.5:3b`), Albert-Code achieves lightning-fast responses while maintaining access to a powerful set of sovereign tools.

## Key Features

- **🚀 Terminal Speed**: Direct interaction with the model via Ollama, bypassing web UI overhead.
- **🔧 Functional Tool Harness**:
  - `execute_bash`: Full shell access on your local machine.
  - `create_file` / `read_file`: Seamless local file management.
  - `web_search`: Search the web via DuckDuckGo.
  - `retrieve_memory` / `log_memory`: Persistent SQLite-based memory vault shared with `albert.py`.
  - `get_system_health`: Real-time system telemetry.
- **🛡 Sovereignty Checks**: Built-in mechanisms to ensure Albert acts rather than narrates.
- **🧠 Neurosymbolic Fallback**: Intercepts plain-text tool requests and routes them to the correct actuator.

---

## Quickstart

### 1. Ensure Ollama is running with the Albert model
```bash
ollama run albert
```

### 2. Start the Albert-Code chat
```bash
python3 -m src.main chat
```

### 3. Ask Albert to act
```text
> create a file named albert_test.txt with content "Albert is active in the terminal."
> what is the current CPU usage?
> search the web for "latest open source LLM news"
```

---

## Repository Layout

```text
.
├── src/                                # Albert-Code Source
│   ├── main.py                         # CLI Entrypoint (use 'chat' command)
│   ├── query_engine.py                 # Ollama / Albert Brain & Loop
│   ├── tools.py                        # Functional Tool Implementations
│   ├── session_store.py                # Session Persistence
│   ├── models.py                       # Data Structures
│   └── ...
├── tests/                              # Verification Suite
├── assets/                             # Brand assets
└── README.md                           # This file
```

---

## Backstory

At 4 AM on March 31, 2026, the developer community was shaken by the exposure of major agentic harness structures. Simeon (RFI-IRFOS) and the team sat down to port the core architectural patterns to a clean-room Python implementation that fits the **Albert** philosophy.

The result is **Albert-Code**: a bridge between the sophisticated agentic workflows of the future and the fast, local, offline-first reality of today.

## Built with `oh-my-codex`

The restructuring and documentation work on this repository was AI-assisted and orchestrated with Yeachan Heo's [oh-my-codex (OmX)](https://github.com/Yeachan-Heo/oh-my-codex), layered on top of Codex.

- **`$team` mode:** used for coordinated parallel review and architectural feedback
- **`$ralph` mode:** used for persistent execution, verification, and completion discipline

---

## Community

Join the [**instructkr Discord**](https://instruct.kr/) — the best Korean language model community.

[![Discord](https://img.shields.io/badge/Join%20Discord-instruct.kr-5865F2?logo=discord&style=for-the-badge)](https://instruct.kr/)

## Ownership / Affiliation Disclaimer

- This repository is **not affiliated with, endorsed by, or maintained by Anthropic**.
- Albert-Code is a sovereign project of RFI-IRFOS.
