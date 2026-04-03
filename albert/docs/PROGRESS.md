# albert. — Progress Log & Roadmap

Last updated: 2026-04-02

---

## What was built

### Core app — albert.py (Streamlit UI)
- Full Streamlit chat interface with dark theme (JetBrains Mono, accent #c8f135)
- Ollama-backed LLM with session management via SQLite (`albert_os.db`)
- Agent loop with tool-calling: web search, bash, file I/O, vault memory, system health, screen capture, SQL
- Library upload (txt/md/json) + TF-IDF search via scikit-learn
- Per-session history, condensation, and auto-titling

### CLI — code-main/src/ (terminal interface)
- `albert` command → ALBERT-CHAT (lite, no tools, conversational)
- `albert code` command → ALBERT-CODE (full agent loop with tool-calling)
- Slash commands: /help /tools /health /clear /memory /bash /makefile
- Session persistence via JSON store
- QueryEnginePort agent loop with neurosymbolic tool-call fallback

### Model — nano.mf (Ollama modelfile)
- Based on gemma3:4b
- Three-mode system: casual / direct answer / deep analysis
- Six few-shot MESSAGE pairs baked in for conversational style
- Temperature 0.72, top_p 0.92

---

## Changes made in this session (2026-04-02)

### Performance
- GPU layers default: 0 → 10 (biggest speed gain)
- CPU threads default: 4 → cpu_count - 2 (6 on ZBook)
- Streamlit streaming batched every 5 tokens (reduces DOM re-renders)
- Agent loop streaming also batched every 5 tokens

### Reliability
- SQL bridge: was routing through sqlite3 shell (`execute_bash`) → now uses Python sqlite3 directly (`execute_sql`) — no shell injection risk
- Neurosymbolic fallback moved before sovereignty override loop (saves one full inference on narration)
- `_needs_agent` keyword list narrowed — removed "list", "what is", "show me", "find" (were routing casual questions into the agent loop)
- Aggressivenaration check in query_engine.py fixed — `(len > 5 and not tool_calls)` condition removed (was marking every conversational reply as narration)

### Code quality
- Module-level `_bg_executor` — replaced throw-away `ThreadPoolExecutor()` in auto_title
- `set_config`, `delete_session` cursor leak fixes
- Inference `opts` defaults now derived from runtime (not hardcoded old values)

### UI/UX (albert.py Streamlit)
- 3-dot bounce typing indicator (replaces single pulsing dot)
- Message fade-in animation (220ms)
- Response stats badge: `1.3s · 47 tok · 12.4 t/s`
- Tool execution pills in agent loop
- Auto-scroll to latest message

### CLI chat (main.py)
- Fixed double output bug (was printing inside submit_message AND in main loop)
- `[simeon]:` / `[albert]:` labels on chat turns (yellow / cyan)

### Model prompt (nano.mf)
- Rewrote system prompt: banned "operational/nominal/processing" vocabulary
- Added concrete few-shot MESSAGE pairs for conversational style
- Rebuilt albert model after each change
- lite mode in query_engine.py no longer injects a competing system prompt (lets nano.mf do its job)

### Documentation & structure
- README_md rewritten with full dependency list, tool table, DB schema, perf guide
- Folder reorganised: docs/, assets/, logs/ created for loose files
- Backups created of all edited files

---

## Known issues / still to do

### Model personality
- gemma3:4b resists persona changes — still sometimes formal despite few-shot examples
- **Next step:** Try `llama3.2:3b-instruct-q4_K_M` as base (already installed on ZBook)
  - `sed -i 's/FROM gemma3:4b/FROM llama3.2:3b-instruct-q4_K_M/' nano.mf && ollama create albert -f nano.mf`

### Kiwix integration
- `retriever.py` exists and `albert.json` references `http://localhost:8080`
- NOT yet wired into `albert.py` or the CLI agent loop
- Would give albert offline Wikipedia access for research queries

### albert.py path fragility
- `DB_PATH = "albert_os.db"`, `LIB_DIR = "library"`, `AGENT_FILES_DIR = "agent files"` are relative
- Fine when launched via Streamlit from the albert/ dir, fragile otherwise
- Fix: use `Path(__file__).parent / "..."` instead of bare strings

### Vault / memory browser
- No UI to browse/search/delete vault entries directly
- Currently you have to ask Albert to retrieve them or use the Streamlit agent
- Could add a dedicated `/vault` panel in the Streamlit sidebar

### PDF support
- `HAS_PDF` / `PdfReader` is imported in albert.py but library upload only handles text
- PDF parsing on upload is wired but not tested

### agent files/ folder
- Contains many reference markdown docs (100+ files)
- Most are not RFI-IRFOS specific — look like imported Claude Code agent docs
- Worth auditing: keep what's relevant, remove what isn't

### Albert Code mode tool-calling
- Works but relies on neurosymbolic fallback often
- Consider adding streaming output in agent mode (currently waits for full response)

### Ternary response parser
- nano.mf defines ternary scoring (+1/0/-1) but the UI doesn't render it specially
- Could detect and highlight ternary sections visually in both CLI and Streamlit

---

## Folder structure

```
albert/
├── albert.py              # Streamlit UI (main app)
├── albert_api.py          # REST API wrapper
├── logger.py              # JSONL conversation logger
├── retriever.py           # Kiwix retriever (not yet wired)
├── nano.mf                # Ollama modelfile for albert:latest
├── albert.json            # Legacy config (model path, kiwix url)
├── albert_os.db           # Main SQLite DB (sessions, messages, vault, config, library)
├── albert_memory.db       # Secondary memory store
├── albert_log.jsonl       # Flat conversation ledger (written by albert_api.py)
├── albert_vault.jsonl     # Flat vault export (written by logger.py / retriever.py)
│
├── code-main/             # Terminal CLI codebase
│   └── src/
│       ├── main.py        # Entry point — chat loop, slash commands
│       ├── query_engine.py # Agent loop, sovereignty check, tool execution
│       ├── tools.py       # Tool implementations for CLI
│       └── ...
│
├── agent files/           # Reference docs (searched via search_library tool)
├── library/               # Uploaded files (via Streamlit library panel)
├── docs/                  # Documentation and text backups
│   ├── README.md
│   ├── PROGRESS.md        # This file
│   ├── notes.txt
│   ├── core_memory.txt
│   └── system_prompt.txt
├── assets/                # Static files
│   └── albert_bi_dashboard_v2.html
├── logs/                  # Runtime logs
│   ├── albert.log
│   └── streamlit.log
└── Backups/               # Snapshots of edited files
```
