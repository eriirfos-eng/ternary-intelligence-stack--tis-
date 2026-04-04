from __future__ import annotations

import base64
import json
import os
import random
import re
import sqlite3
import subprocess
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from functools import lru_cache
from pathlib import Path

import ollama
import requests

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

from .models import PortingBacklog, PortingModule
from .permissions import ToolPermissionContext

from .config import DB_PATH

# ─────────────────────────────────────────────
#  PATHS & CONSTANTS
# ─────────────────────────────────────────────
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
ALBERT_DIR = REPO_ROOT / "albert-agent"
# DB_PATH is imported from config.py

LIB_DIR         = str(ALBERT_DIR / "library")
AGENT_FILES_DIR = str(ALBERT_DIR / "agent files")
TERN_CLI_PATH   = str(REPO_ROOT / "ternlang-root" / "target" / "release" / "ternlang-cli")
MAX_TOOL_CHARS  = 3500

SNAPSHOT_PATH = Path(__file__).resolve().parent / 'reference_data' / 'tools_snapshot.json'

@dataclass(frozen=True)
class ToolExecution:
    name: str
    source_hint: str
    payload: str
    handled: bool
    message: str
    result: object = None


@lru_cache(maxsize=1)
def load_tool_snapshot() -> tuple[PortingModule, ...]:
    if not SNAPSHOT_PATH.exists():
        return ()
    raw_entries = json.loads(SNAPSHOT_PATH.read_text())
    return tuple(
        PortingModule(
            name=entry['name'],
            responsibility=entry['responsibility'],
            source_hint=entry['source_hint'],
            status='mirrored',
        )
        for entry in raw_entries
    )

PORTED_TOOLS = load_tool_snapshot()

# ─────────────────────────────────────────────
#  DATABASE HELPERS
# ─────────────────────────────────────────────
def get_db_conn():
    return sqlite3.connect(DB_PATH, check_same_thread=False)

# ─────────────────────────────────────────────
#  AGENT TOOLS (from albert.py)
# ─────────────────────────────────────────────

def web_search(query: str):
    if not HAS_DDG:
        return "Error: duckduckgo-search not installed."
    try:
        results = []
        with DDGS() as ddgs:
            for r in ddgs.text(query, max_results=5):
                results.append(f"{r.get('title')}: {r.get('body')} ({r.get('href')})")
        return "\n\n".join(results) if results else "No results. Try a more specific query."
    except Exception as e:
        return f"Web search error: {e}"

def fetch_url(url: str):
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

def search_library(query: str):
    conn = get_db_conn()
    c = conn.cursor()
    c.execute("CREATE TABLE IF NOT EXISTS library (id TEXT PRIMARY KEY, filename TEXT, content TEXT, size INTEGER, uploaded_at DATETIME)")
    c.execute("SELECT filename, content FROM library")
    db_files = c.fetchall()
    
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

def execute_bash(command: str):
    try:
        clean = re.sub(r'^\s*\$\s*', '', command)
        r     = subprocess.run(clean, shell=True, text=True, capture_output=True, timeout=30)
        out   = r.stdout or r.stderr
        return f"Exit {r.returncode}:\n{out}" if out else f"Exit {r.returncode}: (no output)"
    except subprocess.TimeoutExpired:
        return "Error: command timed out after 30s."
    except Exception as e:
        return f"Bash execution failed: {e}"

def create_file(path: str, content: str = ""):
    try:
        full = os.path.expanduser(path)
        os.makedirs(os.path.dirname(full) or ".", exist_ok=True)
        with open(full, "w") as f:
            f.write(content)
        return f"File created: {full}"
    except Exception as e:
        return f"File creation failed: {e}"

def read_file(path: str):
    try:
        with open(os.path.expanduser(path), "r") as f:
            return f.read()
    except Exception as e:
        return f"File read failed: {e}"

def log_memory(entry_text: str):
    conn = get_db_conn()
    vid = str(uuid.uuid4())
    now = datetime.now().isoformat()
    c = conn.cursor()
    c.execute("CREATE TABLE IF NOT EXISTS vault (id TEXT PRIMARY KEY, content TEXT, timestamp DATETIME)")
    c.execute("INSERT INTO vault (id, content, timestamp) VALUES (?,?,?)", (vid, entry_text, now))
    conn.commit()
    return f"✓ Committed to vault: {entry_text}"

def retrieve_memory(query: str = ""):
    conn = get_db_conn()
    c = conn.cursor()
    c.execute("CREATE TABLE IF NOT EXISTS vault (id TEXT PRIMARY KEY, content TEXT, timestamp DATETIME)")
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

def pin_core_memory(fact: str):
    conn = get_db_conn()
    c = conn.cursor()
    c.execute("CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)")
    c.execute("SELECT value FROM config WHERE key='core_memory'")
    row = c.fetchone()
    current = row[0] if row else "- User: Simeon"
    new_val = current + f"\n- {fact}"
    c.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('core_memory', ?)", (new_val,))
    conn.commit()
    return f"✓ Pinned to core memory: {fact}"

def ternlang_run(path_or_code: str):
    """Compile and run a .tern file or a raw code snippet via ternlang-cli."""
    if not os.path.exists(TERN_CLI_PATH):
        return f"Error: ternlang-cli not found at {TERN_CLI_PATH}. Please build it first."
    
    run_path = path_or_code
    is_snippet = not path_or_code.endswith(".tern") or not os.path.exists(path_or_code)
    
    if is_snippet:
        run_path = "albert_agent_snippet.tern"
        with open(run_path, "w") as f:
            f.write(path_or_code)
            
    try:
        cmd = f"\"{TERN_CLI_PATH}\" run \"{run_path}\""
        r = subprocess.run(cmd, shell=True, text=True, capture_output=True, timeout=30)
        out = r.stdout or r.stderr
        return f"Ternlang Exit {r.returncode}:\n{out}" if out else f"Ternlang Exit {r.returncode}: (no output)"
    except Exception as e:
        return f"Ternlang execution failed: {e}"
    finally:
        if is_snippet and os.path.exists(run_path):
            try: os.remove(run_path)
            except: pass

TOOL_MAP = {
    'web_search':        web_search,
    'fetch_url':         fetch_url,
    'get_system_health': get_system_health,
    'capture_screen':    capture_screen,
    'search_library':    search_library,
    'execute_bash':      execute_bash,
    'create_file':       create_file,
    'read_file':         read_file,
    'log_memory':        log_memory,
    'retrieve_memory':   retrieve_memory,
    'pin_core_memory':   pin_core_memory,
    'ternlang_run':      ternlang_run,
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
            'Pass an empty string to see recent entries.'
        ),
        'parameters': {'type': 'object', 'properties': {'query': {'type': 'string', 'description': 'Search term or empty string for recent logs.'}}, 'required': ['query']}
    }},
    {'type': 'function', 'function': {
        'name': 'pin_core_memory',
        'description': "Permanently pin a fact to Albert's core identity memory (persists across all sessions).",
        'parameters': {'type': 'object', 'properties': {'fact': {'type': 'string'}}, 'required': ['fact']}
    }},
    {'type': 'function', 'function': {
        'name': 'ternlang_run',
        'description': 'Compile and run a .tern program or a raw ternary code snippet on the BET VM.',
        'parameters': {'type': 'object', 'properties': {'path_or_code': {'type': 'string', 'description': 'File path to a .tern file OR raw ternary source code'}}, 'required': ['path_or_code']}
    }},
]

# ─────────────────────────────────────────────
#  MANAGEMENT
# ─────────────────────────────────────────────

def build_tool_backlog() -> PortingBacklog:
    return PortingBacklog(title='Tool surface', modules=list(PORTED_TOOLS))


def tool_names() -> list[str]:
    # Use actual tool names from the map
    return list(TOOL_MAP.keys())


def get_tool(name: str) -> PortingModule | None:
    # First check mapped tools
    if name in TOOL_MAP:
        return PortingModule(name=name, responsibility="Functional Albert tool", source_hint="src/tools.py", status="functional")
    
    needle = name.lower()
    for module in PORTED_TOOLS:
        if module.name.lower() == needle:
            return module
    return None


def filter_tools_by_permission_context(tools: tuple[PortingModule, ...], permission_context: ToolPermissionContext | None = None) -> tuple[PortingModule, ...]:
    if permission_context is None:
        return tools
    return tuple(module for module in tools if not permission_context.blocks(module.name))


def get_tools(
    simple_mode: bool = False,
    include_mcp: bool = True,
    permission_context: ToolPermissionContext | None = None,
) -> tuple[PortingModule, ...]:
    # Combine functional tools with ported snapshots
    functional = [
        PortingModule(name=name, responsibility="Functional Albert tool", source_hint="src/tools.py", status="functional")
        for name in TOOL_MAP.keys()
    ]
    tools = functional + list(PORTED_TOOLS)
    if simple_mode:
        tools = [module for module in tools if module.name in {'execute_bash', 'read_file', 'create_file'}]
    if not include_mcp:
        tools = [module for module in tools if 'mcp' not in module.name.lower()]
    return filter_tools_by_permission_context(tuple(tools), permission_context)


def find_tools(query: str, limit: int = 20) -> list[PortingModule]:
    needle = query.lower()
    all_tools = get_tools()
    matches = [module for module in all_tools if needle in module.name.lower()]
    return matches[:limit]


from .api_client import AlbertApiClient
client = AlbertApiClient()
...
def execute_tool(name: str, payload: str | dict = '') -> ToolExecution:
    try:
        result = client.run_tool(name, payload)
        return ToolExecution(
            name=name,
            source_hint="Remote API",
            payload=str(payload),
            handled=True,
            message=f"Executed remote tool '{name}'",
            result=result
        )
    except Exception as e:
        return ToolExecution(
            name=name,
            source_hint="Remote API",
            payload=str(payload),
            handled=True,
            message=f"Error executing tool '{name}': {e}"
        )


def render_tool_index(limit: int = 20, query: str | None = None) -> str:
    modules = find_tools(query, limit) if query else list(get_tools()[:limit])
    lines = [f'Tool entries: {len(modules)}', '']
    if query:
        lines.append(f'Filtered by: {query}')
        lines.append('')
    lines.extend(f'- {module.name} [{module.status}] — {module.source_hint}' for module in modules)
    return '\n'.join(lines)
