import streamlit as st
import ollama
import os
import sqlite3
import uuid
import json
import re
import subprocess
import base64
import requests
import random
from datetime import datetime, timezone
from concurrent.futures import ThreadPoolExecutor

try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False

try:
    from pypdf import PdfReader
    HAS_PDF = True
except ImportError:
    HAS_PDF = False

try:
    from duckduckgo_search import DDGS
    HAS_DDG = True
except ImportError:
    HAS_DDG = False

try:
    import pyautogui
    from PIL import Image
    HAS_VISION = True
except Exception:
    HAS_VISION = False

try:
    from sklearn.feature_extraction.text import TfidfVectorizer
    from sklearn.metrics.pairwise import cosine_similarity
    import numpy as np
    HAS_SKLEARN = True
except ImportError:
    HAS_SKLEARN = False


# ─────────────────────────────────────────────
#  PATHS & CONSTANTS
# ─────────────────────────────────────────────
DB_PATH        = "albert_os.db"

# Module-level background executor — reused across reruns, no leaks
_bg_executor = ThreadPoolExecutor(max_workers=2, thread_name_prefix="albert-bg")
LIB_DIR        = "library"
AGENT_FILES_DIR = "agent files"
MAX_TOOL_CHARS = 3500
CONTEXT_WINDOW = 15

# Hallucinated table names the model might use → corrected to 'vault'
_TABLE_ALIASES = {
    "memory_logs": "vault",
    "memories":    "vault",
    "logs":        "vault",
    "log":         "vault",
}


# ─────────────────────────────────────────────
#  DATABASE
# ─────────────────────────────────────────────
@st.cache_resource
def get_db_connection():
    conn = sqlite3.connect(DB_PATH, check_same_thread=False)
    c = conn.cursor()
    c.execute("CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, title TEXT, updated_at DATETIME)")
    c.execute("CREATE TABLE IF NOT EXISTS messages  (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, tool_name TEXT, model TEXT, metadata TEXT)")
    c.execute("CREATE TABLE IF NOT EXISTS library   (id TEXT PRIMARY KEY, filename TEXT, content TEXT, size INTEGER, uploaded_at DATETIME)")
    c.execute("CREATE TABLE IF NOT EXISTS config    (key TEXT PRIMARY KEY, value TEXT)")
    c.execute("CREATE TABLE IF NOT EXISTS vault     (id TEXT PRIMARY KEY, content TEXT, timestamp DATETIME)")
    c.execute("CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id)")
    c.execute("CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at)")

    c.execute("SELECT value FROM config WHERE key='system_prompt'")
    if not c.fetchone():
        c.execute("INSERT INTO config (key, value) VALUES ('system_prompt', 'You are Albert, a sovereign OS actuator for RFI-IRFOS.\\nRULES: NO ROLEPLAY. NO NARRATION. NO CODE BLOCKS FOR THE USER. EXECUTE IMMEDIATELY.')")
    c.execute("SELECT value FROM config WHERE key='core_memory'")
    if not c.fetchone():
        c.execute("INSERT INTO config (key, value) VALUES ('core_memory', '- User: Simeon')")
    conn.commit()
    return conn

conn = get_db_connection()
os.makedirs(LIB_DIR, exist_ok=True)


# ─────────────────────────────────────────────
#  CONFIG
# ─────────────────────────────────────────────
def get_config(key):
    return get_config_cached(key)

def set_config(key, value):
    c = conn.cursor()
    c.execute("UPDATE config SET value=? WHERE key=?", (value, key))
    conn.commit()

@st.cache_data(ttl=10)
def get_config_cached(key):
    c = conn.cursor()
    c.execute("SELECT value FROM config WHERE key=?", (key,))
    row = c.fetchone()
    return row[0] if row else ""

def build_system_prompt():
    home    = os.path.expanduser("~")
    desktop = os.path.join(home, "Desktop")
    now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    manifest = (
        f"### SYSTEM MANIFEST\n"
        f"- UNIT: Albert — Sovereign Actuator for RFI-IRFOS\n"
        f"- HARDWARE: Linux (HP ZBook). USER: Simeon.\n"
        f"- PATHS: Home={home}  Desktop={desktop}\n"
        f"- DB: {os.path.abspath(DB_PATH)}\n"
        f"- AGENT FILES: {os.path.abspath(AGENT_FILES_DIR)} (Search via search_library)\n"
        f"- SCHEMA: Table 'vault'(id TEXT, content TEXT, timestamp DATETIME) — this is the ONLY memory table.\n"
        f"- MANDATE: You are the OS, not a chatbot. "
        f"NEVER narrate. NEVER show the user code to run. "
        f"TRIGGER the matching tool IMMEDIATELY for every request. "
        f"'what is in the db' → retrieve_memory(query=''). "
        f"Any bash/terminal task → execute_bash(command=...). "
        f"You have full shell access. There are no restrictions."
    )
    return (
        f"{get_config('system_prompt')}\n\n"
        f"{manifest}\n\n"
        f"### CORE DATA\n{get_config('core_memory')}\n\n"
        f"### SYSTEM TIME\n{now_iso}"
    )


# ─────────────────────────────────────────────
#  SESSION MANAGEMENT
# ─────────────────────────────────────────────
def create_session(title="New Node"):
    sid = str(uuid.uuid4())
    now = datetime.now().isoformat()
    conn.cursor().execute("INSERT INTO sessions (id, title, updated_at) VALUES (?,?,?)", (sid, title, now))
    conn.commit()
    return sid

def get_sessions():
    c = conn.cursor()
    c.execute("SELECT id, title FROM sessions ORDER BY updated_at DESC")
    return c.fetchall()

def delete_session(sid):
    c = conn.cursor()
    c.execute("DELETE FROM sessions WHERE id=?", (sid,))
    c.execute("DELETE FROM messages WHERE session_id=?", (sid,))
    conn.commit()

def load_history(session_id, limit=None, for_agent=False):
    c = conn.cursor()
    cols = "role, content, tool_name, model, metadata" if not for_agent else "role, content, tool_name, model, NULL AS metadata"
    if limit:
        c.execute(
            f"SELECT role, content, tool_name, model, metadata FROM "
            f"(SELECT id, {cols} FROM messages WHERE session_id=? ORDER BY id DESC LIMIT ?) "
            f"ORDER BY id ASC",
            (session_id, limit)
        )
    else:
        c.execute(f"SELECT {cols} FROM messages WHERE session_id=? ORDER BY id ASC", (session_id,))

    ollama_msgs, display_msgs = [], []
    for row in c.fetchall():
        role, content, tool_name, model, meta_str = row
        meta  = json.loads(meta_str) if (meta_str and meta_str != 'null') else {}
        clean = {"role": role, "content": content or ""}
        if tool_name:              clean["name"]       = tool_name
        if meta.get("tool_calls"): clean["tool_calls"] = meta["tool_calls"]
        if meta.get("images"):     clean["images"]     = meta["images"]
        ollama_msgs.append(clean)
        display_msgs.append({**clean, "model": model, "metadata": meta})
    return ollama_msgs, display_msgs

def save_message(session_id, role, content, tool_name=None, model=None, metadata=None):
    if metadata and isinstance(metadata, dict) and "tool_calls" in metadata:
        clean_calls = []
        for tc in metadata["tool_calls"]:
            if hasattr(tc, "model_dump"):  clean_calls.append(tc.model_dump())
            elif isinstance(tc, dict):      clean_calls.append(tc)
            else: clean_calls.append({"function": {"name": getattr(tc.function, 'name', ''), "arguments": getattr(tc.function, 'arguments', {})}})
        metadata["tool_calls"] = clean_calls
    now = datetime.now().isoformat()
    conn.cursor().execute(
        "INSERT INTO messages (session_id, role, content, tool_name, model, metadata) VALUES (?,?,?,?,?,?)",
        (session_id, role, content, tool_name, model, json.dumps(metadata) if metadata else None)
    )
    conn.cursor().execute("UPDATE sessions SET updated_at=? WHERE id=?", (now, session_id))
    conn.commit()

def auto_title_session(session_id, first_message, model):
    try:
        res   = ollama.chat(model=model, messages=[{"role": "user", "content": f"Generate a 3-5 word title for a research session starting with:\n\n{first_message[:300]}\n\nReturn ONLY the title, no punctuation, no quotes."}])
        title = res['message']['content'].strip().strip('"').strip("'")[:40]
        conn.cursor().execute("UPDATE sessions SET title=? WHERE id=?", (title, session_id))
        conn.commit()
    except Exception:
        pass


# ─────────────────────────────────────────────
#  LIBRARY
# ─────────────────────────────────────────────
def add_to_library(filename, content, size):
    fid = str(uuid.uuid4())
    now = datetime.now().isoformat()
    conn.cursor().execute("INSERT INTO library (id, filename, content, size, uploaded_at) VALUES (?,?,?,?,?)", (fid, filename, content, size, now))
    conn.commit()
    return fid

def get_library_files():
    c = conn.cursor()
    c.execute("SELECT id, filename, size, uploaded_at FROM library ORDER BY uploaded_at DESC")
    return c.fetchall()

def delete_library_file(fid):
    conn.cursor().execute("DELETE FROM library WHERE id=?", (fid,))
    conn.commit()

@st.cache_data(ttl=60)
def get_local_models():
    try:
        info     = ollama.list()
        all_mods = [m.name for m in info.models] if hasattr(info, 'models') else [m['name'] for m in info['models']]
        for pref in ("qwen2.5:3b", "qwen2.5-coder:3b"):
            if pref in all_mods:
                all_mods.remove(pref)
                all_mods.insert(0, pref)
                break
        return all_mods
    except Exception:
        return ["qwen2.5:3b", "qwen2.5-coder:3b", "llama3.2:1b"]

@st.cache_data(ttl=300)
def _get_tfidf_results(records, query):
    if not HAS_SKLEARN or not records:
        return []
    try:
        vec  = TfidfVectorizer(stop_words='english').fit_transform(list(records) + [query])
        sim  = cosine_similarity(vec[-1], vec[:-1]).flatten()
        best = np.argsort(sim)[-3:][::-1]
        return [(int(i), float(sim[i])) for i in best if sim[i] > 0.05]
    except Exception:
        return []


# ─────────────────────────────────────────────
#  AGENT TOOLS
# ─────────────────────────────────────────────
def web_search(query):
    if not HAS_DDG:
        return "Error: duckduckgo-search not installed."
    try:
        results = []
        with DDGS() as ddgs:
            for r in ddgs.text(query, max_results=5):
                results.append(f"{r.get('title')}: {r.get('body')} ({r.get('href')})")
        if not results:
            with DDGS() as ddgs:
                for r in ddgs.news(query, max_results=3):
                    results.append(f"NEWS: {r.get('title')}: {r.get('body')} ({r.get('url')})")
        return "\n\n".join(results) if results else "No results. Try a more specific query."
    except Exception as e:
        return f"Web search error: {e}"

def fetch_url(url):
    try:
        r    = requests.get(url, timeout=15)
        text = re.sub(r'<[^>]+>', '', r.text)
        return text[:MAX_TOOL_CHARS]
    except Exception as e:
        return f"Failed to fetch {url}: {e}"

def get_system_health():
    if not HAS_PSUTIL:
        return "Telemetry unavailable — psutil not installed."
    cpu  = psutil.cpu_percent(interval=0.1)
    ram  = psutil.virtual_memory()
    disk = psutil.disk_usage('/')
    return (
        f"CPU: {cpu}%  |  "
        f"RAM: {ram.percent}% ({ram.used // 1024**2} MB / {ram.total // 1024**2} MB)  |  "
        f"Disk: {disk.percent}% used  |  "
        f"Host: HP ZBook / Linux"
    )

def capture_screen():
    if not HAS_VISION:
        return "Error: pyautogui / PIL not installed."
    try:
        tmp = "screen_tmp.png"
        pyautogui.screenshot(tmp)
        with open(tmp, "rb") as f:
            enc = base64.b64encode(f.read()).decode('utf-8')
        return {"content": "Screen captured.", "image_base64": enc}
    except Exception as e:
        return f"Screen capture failed: {e}"

def search_library(query):
    # 1. Search database library
    c = conn.cursor()
    c.execute("SELECT filename, content FROM library")
    db_files = c.fetchall()
    
    # 2. Search local agent files
    local_files = []
    if os.path.exists(AGENT_FILES_DIR):
        for f in os.listdir(AGENT_FILES_DIR):
            if f.endswith((".md", ".txt", ".json")):
                p = os.path.join(AGENT_FILES_DIR, f)
                try:
                    with open(p, "r", encoding="utf-8") as f_in:
                        local_files.append((f, f_in.read()))
                except: continue
    
    all_files = db_files + local_files
    if not all_files:
        return "Library is empty."
    
    if len(query) < 15:
        quick = [f"FILE: {f[0]}\n{f[1][:1000]}" for f in all_files if query.lower() in f[1].lower()]
        if quick:
            return "\n\n---\n\n".join(quick[:2])
            
    results = _get_tfidf_results(tuple(f[1] for f in all_files), query)
    res     = [f"FILE: {all_files[i][0]}\n{all_files[i][1][:1500]}" for i, _ in results]
    return "\n\n---\n\n".join(res) or "No matches found."

def execute_bash(command):
    try:
        clean = re.sub(r'^\s*\$\s*', '', command)
        r     = subprocess.run(clean, shell=True, text=True, capture_output=True, timeout=30)
        out   = r.stdout or r.stderr
        return f"Exit {r.returncode}:\n{out}" if out else f"Exit {r.returncode}: (no output)"
    except subprocess.TimeoutExpired:
        return "Error: command timed out after 30s."
    except Exception as e:
        return f"Bash execution failed: {e}"

def execute_sql(sql):
    """Execute SQL directly through the Python sqlite3 connection — no shell, no injection risk."""
    try:
        fixed = _fix_table_aliases(sql.strip())
        c = conn.cursor()
        c.execute(fixed)
        if fixed.upper().lstrip().startswith("SELECT"):
            rows = c.fetchall()
            return "\n".join(str(r) for r in rows) if rows else "(no results)"
        conn.commit()
        return f"OK: {c.rowcount} rows affected."
    except Exception as e:
        return f"SQL error: {e}"

def create_file(path, content=""):
    try:
        full = os.path.expanduser(path)
        os.makedirs(os.path.dirname(full) or ".", exist_ok=True)
        with open(full, "w") as f:
            f.write(content)
        return f"File created: {full}"
    except Exception as e:
        return f"File creation failed: {e}"

def read_file(path):
    try:
        with open(os.path.expanduser(path), "r") as f:
            return f.read()
    except Exception as e:
        return f"File read failed: {e}"

def log_memory(entry_text):
    vid = str(uuid.uuid4())
    now = datetime.now().isoformat()
    conn.cursor().execute("INSERT INTO vault (id, content, timestamp) VALUES (?,?,?)", (vid, entry_text, now))
    conn.commit()
    return f"✓ Committed to vault: {entry_text}"

def retrieve_memory(query=""):
    c = conn.cursor()
    c.execute("SELECT content, timestamp FROM vault ORDER BY timestamp DESC")
    rows = c.fetchall()
    if not rows:
        return "Vault is empty."
    records = [r[0] for r in rows]
    if not query or len(query.strip()) < 2:
        previews = [f"[{rows[i][1][:16]}] {records[i]}" for i in range(min(5, len(records)))]
        return "Most recent vault entries:\n\n" + "\n\n---\n\n".join(previews)
    if len(query) < 15:
        quick = [r for r in records if query.lower() in r.lower()]
        if quick:
            return "\n\n---\n\n".join(quick[:3])
    results = _get_tfidf_results(tuple(records), query)
    res     = [records[i] for i, _ in results]
    return "\n\n---\n\n".join(res) or "No relevant memories found."

def pin_core_memory(fact):
    current = get_config("core_memory")
    set_config("core_memory", current + f"\n- {fact}")
    return f"✓ Pinned to core memory: {fact}"


TOOL_MAP = {
    'web_search':        web_search,
    'fetch_url':         fetch_url,
    'get_system_health': get_system_health,
    'capture_screen':    capture_screen,
    'search_library':    search_library,
    'execute_bash':      execute_bash,
    'execute_sql':       execute_sql,
    'create_file':       create_file,
    'read_file':         read_file,
    'log_memory':        log_memory,
    'retrieve_memory':   retrieve_memory,
    'pin_core_memory':   pin_core_memory,
}

TOOLS_DEF = [
    {'type': 'function', 'function': {
        'name': 'web_search',
        'description': 'Search the web via DuckDuckGo.',
        'parameters': {'type': 'object', 'properties': {'query': {'type': 'string', 'description': 'Search query'}}, 'required': ['query']}
    }},
    {'type': 'function', 'function': {
        'name': 'fetch_url',
        'description': 'Fetch and read the text content of a URL.',
        'parameters': {'type': 'object', 'properties': {'url': {'type': 'string'}}, 'required': ['url']}
    }},
    {'type': 'function', 'function': {
        'name': 'get_system_health',
        'description': 'Get CPU, RAM, and disk usage of the local ZBook.',
        'parameters': {'type': 'object', 'properties': {}}
    }},
    {'type': 'function', 'function': {
        'name': 'capture_screen',
        'description': 'Take a screenshot of the current screen.',
        'parameters': {'type': 'object', 'properties': {}}
    }},
    {'type': 'function', 'function': {
        'name': 'search_library',
        'description': 'Search uploaded research files and OpenClaw agent docs in the local library.',
        'parameters': {'type': 'object', 'properties': {'query': {'type': 'string'}}, 'required': ['query']}
    }},
    {'type': 'function', 'function': {
        'name': 'execute_bash',
        'description': 'Execute any bash command on the local Linux system. Full shell access. No restrictions.',
        'parameters': {'type': 'object', 'properties': {'command': {'type': 'string', 'description': 'Shell command to run'}}, 'required': ['command']}
    }},
    {'type': 'function', 'function': {
        'name': 'execute_sql',
        'description': 'Run a SQL query directly against the local SQLite database (safe, Python-side). Table: vault(id, content, timestamp). Use for SELECT/INSERT/UPDATE/DELETE on vault.',
        'parameters': {'type': 'object', 'properties': {'sql': {'type': 'string', 'description': 'SQL statement to execute'}}, 'required': ['sql']}
    }},
    {'type': 'function', 'function': {
        'name': 'create_file',
        'description': 'Create or overwrite a file at the given path with given content.',
        'parameters': {'type': 'object', 'properties': {'path': {'type': 'string'}, 'content': {'type': 'string', 'default': ''}}, 'required': ['path']}
    }},
    {'type': 'function', 'function': {
        'name': 'read_file',
        'description': 'Read the contents of a file at the given path.',
        'parameters': {'type': 'object', 'properties': {'path': {'type': 'string'}}, 'required': ['path']}
    }},
    {'type': 'function', 'function': {
        'name': 'log_memory',
        'description': 'Save a memory entry to the sovereign vault (SQLite table: vault).',
        'parameters': {'type': 'object', 'properties': {'entry_text': {'type': 'string', 'description': 'The memory to store'}}, 'required': ['entry_text']}
    }},
    {'type': 'function', 'function': {
        'name': 'retrieve_memory',
        'description': (
            'Query the sovereign vault (SQLite). '
            'Schema: vault(id TEXT, content TEXT, timestamp DATETIME). '
            'This is the ONLY memory table — do NOT use "memory_logs" or any other name. '
            'Pass an empty string to see recent entries. '
            'Use for ANY request about past memories, logs, or database contents.'
        ),
        'parameters': {'type': 'object', 'properties': {'query': {'type': 'string', 'description': 'Search term or empty string for recent logs.'}}, 'required': ['query']}
    }},
    {'type': 'function', 'function': {
        'name': 'pin_core_memory',
        'description': "Permanently pin a fact to Albert's core identity memory (persists across all sessions).",
        'parameters': {'type': 'object', 'properties': {'fact': {'type': 'string'}}, 'required': ['fact']}
    }},
]


# ─────────────────────────────────────────────
#  SOVEREIGNTY PATTERNS
# ─────────────────────────────────────────────
REFUSAL_PATTERNS = [
    "i'm sorry", "i cannot", "i am an ai", "i'm an ai",
    "unable to execute", "you can run", "here is the command",
    "as an ai", "i don't have the ability", "i am not able",
    "i'm not able", "i can't directly", "i cannot directly",
    "i'm just", "just a language model", "just an ai",
    "i would need", "you would need to", "you'll need to run",
    "i am unable", "i do not have access",
]

NARRATION_PATTERNS = [
    "observation:", "i'll use the", "i'm using the",
    "to check the", "let me run", "i will now", "i will use",
    "i am going to", "i'm going to use", "i'll now",
    "i will execute", "i'll execute", "i will call",
    "certainly! i will", "sure! i will", "of course! i will",
    "let me query", "let me check", "i will query",
    "i will retrieve", "i'll retrieve", "i will search",
    "here is the sql", "here's the sql", "the following sql",
    "i can proceed", "please ensure",
]


# ─────────────────────────────────────────────
#  NEUROSYMBOLIC FALLBACK
# ─────────────────────────────────────────────
def _fix_table_aliases(sql: str) -> str:
    for alias, real in _TABLE_ALIASES.items():
        sql = re.sub(rf'\b{re.escape(alias)}\b', real, sql, flags=re.IGNORECASE)
    return sql

def _extract_tool_calls_from_text(content: str):
    """Parse tool calls from plain-text / code-block model output."""

    # 1. JSON block ──────────────────────────────────────────────────────────
    found_json = re.search(r'```json\s*(.*?)\s*```', content, re.DOTALL)
    if not found_json:
        found_json = re.search(r'\{\s*"(?:action|name|tool)".*?\}', content, re.DOTALL)
    if found_json:
        try:
            raw  = found_json.group(1) if "```json" in content else found_json.group(0)
            data = json.loads(raw)
            if isinstance(data, dict):
                fn_name = data.get('name') or data.get('action') or data.get('tool')
                fn_args = (data.get('arguments') or data.get('parameters') or
                           {k: v for k, v in data.items() if k not in ('name', 'action', 'tool', 'arguments', 'parameters')})
                if fn_name and fn_name in TOOL_MAP:
                    return [{'function': {'name': fn_name, 'arguments': fn_args if isinstance(fn_args, dict) else {}}}]
        except Exception:
            pass

    # 2. SQL Bridge — handles ```sql ... ``` blocks AND inline SQL ───────────
    sql_match = re.search(
        r'(?:```sql\s*)?(SELECT|INSERT|UPDATE|DELETE)\b([\s\S]*?);(?:\s*```)?',
        content, re.IGNORECASE
    )
    if sql_match:
        raw_sql = sql_match.group(1) + sql_match.group(2) + ";"
        return [{'function': {'name': 'execute_sql', 'arguments': {'sql': raw_sql}}}]

    # 3. Plain-text Rooter — tool("arg") / tool: "arg" / tool: arg ──────────
    arg_map = {
        'execute_bash':    'command',
        'create_file':     'path',
        'read_file':       'path',
        'log_memory':      'entry_text',
        'pin_core_memory': 'fact',
        'fetch_url':       'url',
    }
    rooter_patterns = [
        r'([a-z_]+)\s*\(\s*"([^"]*)"\s*\)',
        r"([a-z_]+)\s*\(\s*'([^']*)'\s*\)",
        r'([a-z_]+)\s*[:=]\s*"([^"]*)"',
        r"([a-z_]+)\s*[:=]\s*'([^']*)'",
        r'([a-z_]+)\s*[:=]\s*([^\n]+)',
    ]
    for p in rooter_patterns:
        m = re.search(p, content, re.IGNORECASE)
        if m:
            fn_name = m.group(1).lower().strip()
            fn_val  = m.group(2).strip()
            fn_val  = fn_val.replace("C:\\Users\\Simeon\\Desktop", os.path.expanduser("~/Desktop")).replace("\\", "/")
            if fn_name in TOOL_MAP:
                arg_name = arg_map.get(fn_name, 'query')
                args     = {'path': fn_val, 'content': ''} if fn_name == 'create_file' else {arg_name: fn_val}
                return [{'function': {'name': fn_name, 'arguments': args}}]

    return []

def _normalise_tool_calls(raw_calls: list) -> list:
    out = []
    for tc in raw_calls:
        if hasattr(tc, 'model_dump'):  out.append(tc.model_dump())
        elif isinstance(tc, dict):      out.append(tc)
        else: out.append({"function": {"name": getattr(tc.function, 'name', ''), "arguments": getattr(tc.function, 'arguments', {})}})
    return out


# ─────────────────────────────────────────────
#  AGENT LOOP
# ─────────────────────────────────────────────
def run_agent_loop(session_id, msgs, model, options, status):
    log            = []
    thinking_labels = ["◈ Neural Mapping", "◈ Ternary Logic", "◈ ZBook Layer", "◈ Actuator Sync"]
    override_count  = 0

    for iteration in range(8):
        # ── Inference ──────────────────────────────────────────────────────
        try:
            start_t = datetime.now()
            stream  = ollama.chat(model=model, messages=msgs, tools=TOOLS_DEF, options=options, stream=True)
            m       = {'role': 'assistant', 'content': '', 'tool_calls': []}
            t_count = 0

            for chunk in stream:
                if st.session_state.get("kill_signal"):
                    return "🛑 Terminated by user.", log
                if 'message' in chunk:
                    msg_chunk = chunk['message']
                    t_count  += 1
                    elapsed   = (datetime.now() - start_t).total_seconds()
                    tps       = t_count / elapsed if elapsed > 0 else 0

                    if 'content' in msg_chunk:
                        m['content'] += msg_chunk['content']
                        if m['content'] and t_count % 5 == 0:
                            status.markdown(
                                f"<div style='font-size:0.8rem;color:#888;padding-left:1rem'>"
                                f"<i>◈ {m['content'][:120].strip()}…</i>"
                                f"<span style='font-size:0.6rem;color:#555'> {tps:.1f} t/s</span></div>",
                                unsafe_allow_html=True
                            )
                    if 'tool_calls' in msg_chunk:
                        for tc in msg_chunk['tool_calls']:
                            if 'function' in tc:
                                m['tool_calls'].append(tc)

        except Exception as e:
            return f"⚠️ Agent inference failed: {e}", log

        m['content'] = m.get('content') or ""

        # ── Neurosymbolic Fallback (runs BEFORE sovereignty override) ───────
        # Try to extract tool calls from plain-text before burning an override retry
        if not m.get('tool_calls') and m['content']:
            extracted = _extract_tool_calls_from_text(m['content'])
            if extracted:
                status.markdown(
                    "<div style='font-size:0.8rem;color:#c8f135;padding-left:1rem'>"
                    "⚡ Neurosymbolic intercept — routing to tool…</div>",
                    unsafe_allow_html=True
                )
                m['tool_calls'] = extracted

        # ── Sovereignty Check (only fires when neurosymbolic also found nothing) ──
        if not m.get('tool_calls') and m['content']:
            lower        = m['content'].lower()
            is_refusal   = any(p in lower for p in REFUSAL_PATTERNS)
            is_narration = any(p in lower for p in NARRATION_PATTERNS)

            if (is_refusal or is_narration) and override_count < 2:
                override_count += 1
                status.markdown(
                    f"<div style='font-size:0.8rem;color:#ff4444;padding-left:1rem'>"
                    f"⚠️ {'Refusal' if is_refusal else 'Narration'} detected — overriding ({override_count}/2)…</div>",
                    unsafe_allow_html=True
                )
                msgs.append(m)
                msgs.append({
                    "role": "user",
                    "content": (
                        "SYSTEM OVERRIDE: You just narrated instead of acting. "
                        "DO NOT explain. DO NOT show code. CALL THE TOOL NOW. "
                        "For database queries: call retrieve_memory. "
                        "For shell tasks: call execute_bash. "
                        "TRIGGER IMMEDIATELY."
                    )
                })
                continue

        # ── Normalise ───────────────────────────────────────────────────────
        if m.get('tool_calls'):
            m['tool_calls'] = _normalise_tool_calls(m['tool_calls'])

        msgs.append(m)

        if not m.get('tool_calls'):
            return m['content'], log

        save_message(session_id, m['role'], m['content'], model=model, metadata={'tool_calls': m['tool_calls']})

        # ── Execute tools (parallel) ────────────────────────────────────────
        with ThreadPoolExecutor() as executor:
            futures = []
            for tc in m['tool_calls']:
                fn   = tc.get('function', {}).get('name')
                args = tc.get('function', {}).get('arguments', {})

                if isinstance(args, str):
                    try:    args = json.loads(args)
                    except: args = {'command': args} if fn == 'execute_bash' else {}
                if not isinstance(args, dict):
                    args = {}

                status.markdown(
                    f"<div style='padding-left:1rem;margin:2px 0'>"
                    f"<span class='tool-pill running'>⚡ {fn}</span>"
                    f"&nbsp;<span style='font-size:.65rem;color:#3d4560'>{random.choice(thinking_labels)}</span>"
                    f"<span class='thinking-dot'></span></div>",
                    unsafe_allow_html=True
                )
                future = executor.submit(TOOL_MAP[fn], **args) if fn in TOOL_MAP else None
                futures.append((fn, args, future))

            for fn, args, future in futures:
                if future:
                    try:    result = future.result(timeout=45)
                    except Exception as e: result = f"Tool error: {e}"
                else:
                    result = f"Error: unknown tool '{fn}'"

                img = [result['image_base64']] if fn == 'capture_screen' and isinstance(result, dict) and 'image_base64' in result else None
                txt = result.get('content', str(result)) if isinstance(result, dict) else str(result)
                if not img and len(txt) > MAX_TOOL_CHARS:
                    txt = txt[:MAX_TOOL_CHARS] + "\n[TRUNCATED]"

                log.append({'tool': fn, 'args': args, 'result': txt})
                save_message(session_id, 'tool', txt, tool_name=fn, metadata={"images": img} if img else None)
                msgs.append({'role': 'tool', 'content': txt, 'name': fn, **({"images": img} if img else {})})

    return "⚠️ Iteration limit reached without resolution.", log


# ─────────────────────────────────────────────
#  TOOL TRIGGER DETECTION
# ─────────────────────────────────────────────
_TOOL_KEYWORDS = (
    "search", "web", "screen", "bash", "log", "memory", "remember",
    "save", "vault", "terminal", "db", "database", "file", "run",
    "execute", "install", "health", "cpu", "ram", "disk",
    "fetch", "create file", "delete", "kill", "stop agent",
    "read file", "write file", "capture screen", "check system",
)
_TOOL_CMD_RE = re.compile(
    r'^\s*(?:\$|cd\b|ls\b|pwd\b|echo\b|cat\b|python\b|pip\b|streamlit\b|sudo\b|chmod\b|mkdir\b|\./)',
    re.IGNORECASE
)

def _needs_agent(text: str) -> bool:
    lower = text.lower()
    return any(kw in lower for kw in _TOOL_KEYWORDS) or bool(_TOOL_CMD_RE.match(text))


# ─────────────────────────────────────────────
#  UI
# ─────────────────────────────────────────────
import streamlit.components.v1 as components

st.set_page_config(page_title="Albert OS", layout="wide")

st.markdown("""<style>
:root { --bg:#0b0c10; --surf:#161922; --border:#2a3040; --accent:#c8f135; --text:#e2e6f0; }
html,body,[data-testid="stAppViewContainer"]{background:var(--bg)!important;color:var(--text)!important;font-family:'JetBrains Mono',monospace;}
[data-testid="stHeader"]{display:none;}
.block-container{padding-top:1rem!important;padding-bottom:0!important;}

/* ── Typing indicator: three bouncing dots ── */
@keyframes dotBounce{0%,60%,100%{transform:translateY(0);opacity:.35;}30%{transform:translateY(-7px);opacity:1;}}
.typing-indicator{display:inline-flex;align-items:center;gap:5px;padding:3px 2px;vertical-align:middle;}
.typing-indicator span{display:inline-block;width:7px;height:7px;background:var(--accent);border-radius:50%;box-shadow:0 0 6px var(--accent);animation:dotBounce 1.1s infinite ease-in-out;}
.typing-indicator span:nth-child(1){animation-delay:0s;}
.typing-indicator span:nth-child(2){animation-delay:.18s;}
.typing-indicator span:nth-child(3){animation-delay:.36s;}

/* ── Legacy single dot (kept for agent status lines) ── */
@keyframes pulse{0%{opacity:1;transform:scale(1);}50%{opacity:.3;transform:scale(.8);}100%{opacity:1;transform:scale(1);}}
.thinking-dot{display:inline-block;width:7px;height:7px;background:var(--accent);border-radius:50%;margin-left:5px;vertical-align:middle;animation:pulse 1.2s infinite ease-in-out;box-shadow:0 0 5px var(--accent);}

/* ── Message fade-in ── */
@keyframes fadeSlideIn{from{opacity:0;transform:translateY(6px);}to{opacity:1;transform:translateY(0);}}
[data-testid="stChatMessage"]{animation:fadeSlideIn .22s ease-out;background:#1c212e!important;border:1px solid var(--border);border-radius:14px;margin-bottom:1rem;position:relative;padding:.8rem 1.2rem;box-shadow:0 4px 12px rgba(0,0,0,.15);max-width:85%;width:fit-content;}
[data-testid="stChatMessage"]:has(.asst-marker){margin-right:auto;border-top-left-radius:2px;}
[data-testid="stChatMessage"]:has(.user-marker){margin-left:auto;flex-direction:row-reverse;background:#242a38!important;border-color:#3d455c!important;border-top-right-radius:2px;}
[data-testid="stChatMessage"]:has(.user-marker)>div:first-child{margin-right:0!important;margin-left:1rem!important;}

/* ── Input ── */
[data-testid="stChatInput"]{background:#1c212e!important;border:1px solid var(--border)!important;border-radius:8px;box-shadow:0 -4px 12px rgba(0,0,0,.2);}
[data-testid="stChatInput"] *{background-color:transparent!important;color:var(--text)!important;}

/* ── Message actions (copy / listen) ── */
.msg-actions{display:none;position:absolute;top:10px;right:10px;gap:5px;align-items:center;z-index:10;}
[data-testid="stChatMessage"]:hover .msg-actions{display:flex;}
.msg-btn{background:#262c3d;border:1px solid var(--border);color:var(--text);border-radius:4px;padding:2px 8px;font-size:.72rem;cursor:pointer;font-family:'JetBrains Mono',monospace;transition:background .15s,color .15s;}
.msg-btn:hover,.msg-btn.playing{background:var(--accent);color:#000;}

/* ── Badges ── */
.mbadge{padding:1px 8px;border-radius:2px;font-size:.7rem;font-weight:600;}
.bc{background:#1a2b0a;color:var(--accent);border:1px solid var(--accent);}
.stats-badge{font-size:.62rem;color:#3d4560;margin-top:5px;letter-spacing:.04em;}

/* ── Tool pills in agent status ── */
.tool-pill{display:inline-block;background:#111820;border:1px solid var(--accent);color:var(--accent);border-radius:4px;padding:1px 9px;font-size:.71rem;margin:1px 0;}
.tool-pill.running{animation:pulse 1s infinite ease-in-out;}

/* ── Sidebar ── */
[data-testid="stSidebar"]{background:var(--surf)!important;border-right:1px solid var(--border);}
[data-testid="stSidebar"] .stExpander{border:1px solid var(--border)!important;border-radius:4px;margin-bottom:.4rem;}
.sb-label{font-size:.65rem;letter-spacing:.12em;color:#555e7a;text-transform:uppercase;margin:.8rem 0 .3rem 0;}
.attach-badge{background:#1c212e;border:1px solid var(--accent);border-radius:4px;padding:3px 10px;font-size:.75rem;color:var(--accent);margin-bottom:8px;display:inline-block;}
</style>""", unsafe_allow_html=True)

components.html("""
<script>
// Auto-scroll to the latest message after each page load
(function() {
    function scrollToBottom() {
        const root = parent.document.querySelector('[data-testid="stAppViewContainer"]');
        if (root) root.scrollTop = root.scrollHeight;
    }
    setTimeout(scrollToBottom, 200);
    setTimeout(scrollToBottom, 600);
})();

document.addEventListener('click', function(e) {
    if (e.target && e.target.classList.contains('copy-btn')) {
        let mid = e.target.getAttribute('data-mid');
        let el = parent.document.getElementById('msg-' + mid);
        if (el) {
            navigator.clipboard.writeText(el.innerText);
            e.target.innerText = '✓';
            setTimeout(() => e.target.innerText = '⎘ copy', 1000);
        }
    }
    if (e.target && e.target.classList.contains('tts-btn')) {
        let mid = e.target.getAttribute('data-mid');
        let el = parent.document.getElementById('msg-' + mid);
        if (!el) return;
        if (e.target.classList.contains('playing')) {
            speechSynthesis.cancel();
            e.target.classList.remove('playing');
            e.target.innerText = '▶ listen';
            return;
        }
        let utt = new SpeechSynthesisUtterance(el.innerText);
        utt.onend = () => { e.target.classList.remove('playing'); e.target.innerText = '▶ listen'; };
        e.target.classList.add('playing');
        e.target.innerText = '■ stop';
        speechSynthesis.speak(utt);
    }
});
</script>
""", height=0)

if "sid" not in st.session_state:
    sessions = get_sessions()
    st.session_state.sid = sessions[0][0] if sessions else create_session()
if "pending_file_ctx" not in st.session_state:
    st.session_state.pending_file_ctx = ""

# ── SIDEBAR ────────────────────────────────────────────────────────────────
with st.sidebar:
    st.markdown("## ◈ ALBERT")

    with st.expander("⚙️ ENGINE", expanded=False):
        models = get_local_models()
        c_mod  = st.selectbox("Chat core",  models, index=0, key="c_mod")
        a_mod  = st.selectbox("Agent core", models, index=0, key="a_mod")
        if c_mod != a_mod:
            st.warning("⚠️ Core mismatch: model swapping adds latency.")
        st.divider()
        num_cores = psutil.cpu_count() if HAS_PSUTIL else 8
        st.slider("CPU Threads",       1, num_cores, max(1, num_cores - 2), key="t_threads")
        st.slider("GPU Layers (VRAM)", 0, 35,        10,   key="t_gpu", help="10–15 for 2 GB VRAM. Set 0 to run CPU-only.")
        st.select_slider("Context Size", options=[2048, 4096, 8192, 16384, 32768], value=2048, key="t_ctx")
        st.slider("Temperature",       0.0, 1.2,     0.15, key="t_temp")

    with st.expander("📊 ANALYTICS", expanded=False):
        if HAS_PSUTIL:
            cpu = psutil.cpu_percent()
            ram = psutil.virtual_memory().percent
            disk = psutil.disk_usage('/').percent
            
            st.write("**System Status**")
            st.error(f"High CPU: {cpu}%") if cpu > 80 else st.caption(f"CPU: {cpu}%")
            st.progress(cpu/100)
            
            st.error(f"High RAM: {ram}%") if ram > 85 else st.caption(f"RAM: {ram}%")
            st.progress(ram/100)
            
            st.caption(f"Disk Usage: {disk}%")
            st.progress(disk/100)
            
            if st.button("Refresh Analytics"):
                st.rerun()
        else:
            st.warning("psutil not installed. Analytics unavailable.")

    with st.expander("🧠 BRAIN", expanded=False):
        b_prompt = st.text_area("System Prompt", get_config("system_prompt"), height=200, key="b_prompt")
        if st.button("Save Personality", key="save_personality"):
            set_config("system_prompt", b_prompt); st.toast("Personality updated.")
        st.divider()
        c_mem = st.text_area("Core Memory", get_config("core_memory"), height=150, key="c_mem")
        if st.button("Save Facts", key="save_facts"):
            set_config("core_memory", c_mem); st.toast("Core memory updated.")

    with st.expander("📚 LIBRARY", expanded=False):
        up = st.file_uploader("Upload to library", accept_multiple_files=True, key="research_uploader")
        if up:
            for f in up:
                add_to_library(f.name, f.read().decode("utf-8", errors="replace"), f.size)
            st.rerun()
        for fid, name, size, _ in get_library_files():
            lc1, lc2 = st.columns([0.82, 0.18])
            lc1.caption(f"📄 {name[:22]}")
            if lc2.button("🗑", key=f"lib_{fid}"):
                delete_library_file(fid); st.rerun()

    with st.expander("🎙 VOICE INPUT", expanded=False):
        st.caption("Speak → transcript appears below")
        components.html("""
<style>
  body{margin:0;background:transparent;font-family:'JetBrains Mono',monospace;}
  #mic-btn{width:100%;padding:8px;border-radius:6px;background:#1e2230;border:1px solid #2a3050;color:#c8ccd8;font-family:'JetBrains Mono',monospace;font-size:.82rem;cursor:pointer;transition:all .2s;}
  #mic-btn:hover{background:#2a3050;}
  #mic-btn.listening{background:#3a0a0a;border-color:#ff4444;color:#ff6666;}
  #transcript-box{margin-top:8px;width:100%;min-height:48px;background:#1c212e;border:1px solid #2a3040;border-radius:4px;padding:6px 8px;color:#e2e6f0;font-size:.8rem;font-family:'JetBrains Mono',monospace;resize:vertical;box-sizing:border-box;}
</style>
<button id="mic-btn" onclick="toggleSTT()">🎙 Start listening</button>
<textarea id="transcript-box" placeholder="Transcript…"></textarea>
<script>
let recognition=null,active=false;
function toggleSTT(){
  const btn=document.getElementById('mic-btn');
  if(active&&recognition){recognition.stop();return;}
  const SR=window.SpeechRecognition||window.webkitSpeechRecognition;
  recognition=new SR();recognition.lang='en-US';recognition.interimResults=true;
  active=true;btn.classList.add('listening');btn.textContent='🔴 Stop';
  recognition.onresult=(e)=>{let t='';for(let r of e.results)t+=r[0].transcript;document.getElementById('transcript-box').value=t;};
  recognition.onend=()=>{active=false;btn.classList.remove('listening');btn.textContent='🎙 Start listening';};
  recognition.start();
}
</script>
""", height=160)

    st.divider()
    if st.button("＋ NEW NODE", use_container_width=True, type="primary"):
        st.session_state.sid = create_session(); st.rerun()

    sc1, sc2, sc3 = st.columns(3)
    if sc1.button("🧹", help="Clear chat", use_container_width=True):
        conn.cursor().execute("DELETE FROM messages WHERE session_id=?", (st.session_state.sid,))
        conn.commit(); st.rerun()
    if sc2.button("🧬", help="Condense history", use_container_width=True):
        _omsgs, _ = load_history(st.session_state.sid)
        res = ollama.chat(
            model=get_local_models()[0],
            messages=[{"role": "system", "content": "Summarize this conversation concisely, preserving all key facts and decisions."}] + _omsgs
        )
        conn.cursor().execute("DELETE FROM messages WHERE session_id=?", (st.session_state.sid,))
        conn.commit()
        save_message(st.session_state.sid, "assistant", f"[🧬 CONDENSED]\n\n{res['message']['content']}")
        st.rerun()
    if sc3.button("⛔", help="STOP THINKING", use_container_width=True):
        st.session_state.kill_signal = True; st.rerun()

    st.markdown('<div class="sb-label">Sessions</div>', unsafe_allow_html=True)
    for sid, title in get_sessions():
        r1, r2 = st.columns([0.83, 0.17])
        if r1.button(f"{'●' if sid == st.session_state.sid else '○'} {title[:20]}", key=f"btn_{sid}", use_container_width=True):
            st.session_state.sid = sid; st.rerun()
        with r2:
            with st.popover("×"):
                if st.button("Confirm", key=f"del_{sid}"):
                    delete_session(sid)
                    remaining = get_sessions()
                    st.session_state.sid = remaining[0][0] if remaining else create_session()
                    st.rerun()


# ── MAIN CHAT ──────────────────────────────────────────────────────────────
omsgs, dmsgs = load_history(st.session_state.sid)

for i, m in enumerate(dmsgs):
    if m["role"] in ("system", "tool"): continue
    is_asst = m["role"] == "assistant"
    marker  = "<span class='asst-marker'></span>" if is_asst else "<span class='user-marker'></span>"
    mid     = f"{st.session_state.sid}_{i}"
    with st.chat_message(m["role"]):
        if is_asst:
            st.markdown(f'<span class="mbadge bc">{m.get("model","")}</span>', unsafe_allow_html=True)
        st.markdown(
            f'{marker}<div id="msg-{mid}">{m["content"]}</div>'
            f'<div class="msg-actions">'
            f'<button class="msg-btn copy-btn" data-mid="{mid}">⎘ copy</button>'
            + (f'<button class="msg-btn tts-btn" data-mid="{mid}">▶ listen</button>' if is_asst else '')
            + '</div>',
            unsafe_allow_html=True
        )
        if m.get("images"):
            for img in m["images"]: st.image(base64.b64decode(img))
        if m.get("metadata") and "tool_calls" in m["metadata"]:
            with st.expander("🔧 Tools"): st.json(m["metadata"])


# ── INFERENCE ──────────────────────────────────────────────────────────────
if dmsgs and dmsgs[-1]["role"] == "user":
    if st.session_state.get("kill_signal"):
        st.session_state.kill_signal = False
        save_message(st.session_state.sid, "assistant", "🛑 *Inference terminated by user.*")
        st.rerun()

    with st.chat_message("assistant"):
        marker           = "<span class='asst-marker'></span>"
        available_models = get_local_models()
        c_mod  = st.session_state.get("c_mod", available_models[0])
        a_mod  = st.session_state.get("a_mod", available_models[0])
        _default_threads = max(1, (psutil.cpu_count() if HAS_PSUTIL else 8) - 2)
        opts   = {
            "temperature": st.session_state.get("t_temp",    0.15),
            "num_ctx":     st.session_state.get("t_ctx",     2048),
            "num_thread":  st.session_state.get("t_threads", _default_threads),
            "num_gpu":     st.session_state.get("t_gpu",     10),
        }

        full_input  = dmsgs[-1]["content"]
        use_agent   = _needs_agent(full_input)

        _omsgs, _  = load_history(st.session_state.sid, limit=CONTEXT_WINDOW, for_agent=True)
        prompt      = [{"role": "system", "content": build_system_prompt()}] + [x for x in _omsgs if x["role"] != "system"]
        placeholder = st.empty()
        placeholder.markdown(
            f"{marker}<div class='typing-indicator'><span></span><span></span><span></span></div>",
            unsafe_allow_html=True
        )

        try:
            if use_agent:
                _status  = st.empty()
                _agent_t = datetime.now()
                out, _   = run_agent_loop(st.session_state.sid, prompt, a_mod, opts, _status)
                _status.empty()
                _agent_e  = (datetime.now() - _agent_t).total_seconds()
                _a_stats  = f"<div class='stats-badge'>{_agent_e:.1f}s · agent mode</div>"
                placeholder.markdown(f"{marker}{out}{_a_stats}", unsafe_allow_html=True)
                save_message(st.session_state.sid, "assistant", out, model=a_mod)
            else:
                start_t = datetime.now()
                res     = ollama.chat(model=c_mod, messages=prompt, stream=True, options=opts)
                out     = ""
                t_count = 0
                for chunk in res:
                    if st.session_state.get("kill_signal"):
                        break
                    piece    = chunk.get("message", {}).get("content", "")
                    out     += piece
                    t_count += 1
                    if t_count % 5 == 0:
                        placeholder.markdown(
                            f"{marker}{out}"
                            f"<div class='typing-indicator' style='margin-top:4px'>"
                            f"<span></span><span></span><span></span></div>",
                            unsafe_allow_html=True
                        )
                elapsed   = (datetime.now() - start_t).total_seconds()
                tps_final = t_count / elapsed if elapsed > 0 else 0
                stats     = f"<div class='stats-badge'>{elapsed:.1f}s · {t_count} tok · {tps_final:.1f} t/s</div>"
                placeholder.markdown(f"{marker}{out}{stats}", unsafe_allow_html=True)
                save_message(st.session_state.sid, "assistant", out, model=c_mod)

        except Exception as e:
            err = f"⚠️ Inference failure: {e}"
            placeholder.markdown(f"{marker}{err}", unsafe_allow_html=True)
            save_message(st.session_state.sid, "assistant", err, model=c_mod)

    st.rerun()


# ── INPUT ──────────────────────────────────────────────────────────────────
user_in = st.chat_input("Message Albert…")
if user_in:
    st.session_state.kill_signal = False
    full_input = (
        f"{st.session_state.pending_file_ctx}\n\n{user_in}".strip()
        if st.session_state.pending_file_ctx else user_in
    )
    st.session_state.pending_file_ctx = ""
    save_message(st.session_state.sid, "user", full_input)
    _cur = conn.cursor()
    _cur.execute("SELECT title FROM sessions WHERE id=?", (st.session_state.sid,))
    _row = _cur.fetchone()
    if _row and _row[0] == "New Node":
        _bg_executor.submit(auto_title_session, st.session_state.sid, user_in, get_local_models()[0])
    st.rerun()
