from __future__ import annotations

import json
import os
import re
import sqlite3
from dataclasses import dataclass, field
from datetime import datetime, timezone
from uuid import uuid4
from pathlib import Path

import ollama

from .commands import build_command_backlog
from .models import PermissionDenial, UsageSummary
from .port_manifest import PortManifest, build_port_manifest
from .session_store import StoredSession, load_session, save_session
from .tools import TOOLS_DEF, execute_tool, build_tool_backlog
from .transcript import TranscriptStore

# ─────────────────────────────────────────────
#  PATHS & CONSTANTS (Sync with tools.py)
# ─────────────────────────────────────────────
ALBERT_DIR = Path("/home/eri-irfos/Desktop/albert")
DB_PATH    = str(ALBERT_DIR / "albert_os.db")

@dataclass(frozen=True)
class QueryEngineConfig:
    max_turns: int = 8
    max_budget_tokens: int = 4000
    compact_after_turns: int = 12
    structured_output: bool = False
    structured_retry_limit: int = 2
    model: str = "albert:latest"
    lite_mode: bool = False

@dataclass(frozen=True)
class TurnResult:
    prompt: str
    output: str
    matched_commands: tuple[str, ...]
    matched_tools: tuple[str, ...]
    permission_denials: tuple[PermissionDenial, ...]
    usage: UsageSummary
    stop_reason: str

def get_config(key: str) -> str:
    try:
        conn = sqlite3.connect(DB_PATH)
        c = conn.cursor()
        c.execute("CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)")
        c.execute("SELECT value FROM config WHERE key=?", (key,))
        row = c.fetchone()
        conn.close()
        return row[0] if row else ""
    except:
        return ""

def build_system_prompt(lite: bool = False):
    home    = os.path.expanduser("~")
    desktop = os.path.join(home, "Desktop")
    now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    if lite:
        # Lite / chat mode: keep it minimal and DO NOT use any gear/mode terminology.
        # The model echoes back whatever labels it sees in the system prompt.
        # "casual → short and dry" without naming any mode is enough.
        return (
            "You are Albert. Research agent for RFI-IRFOS, Graz. Operator: Simeon.\n"
            "Casual messages (hey, what's up, how you doing): reply with 2-3 sentences, dry wit, JARVIS-style attitude. "
            "Not terse single words. Not corporate. Keep the conversation moving — ask what he's working on.\n"
            "Questions: answer directly, no padding.\n"
            "Analysis requests: use structured output.\n"
            "Never say 'How can I assist you today?' Never output mode/gear labels.\n"
            f"SESSION TIME: {now_iso}"
        )

    # Code / agent mode: inject full actuator context + tools mandate
    core_m = get_config('core_memory') or "- User: Simeon"
    return (
        "You are Albert — Sovereign Actuator for RFI-IRFOS. Operator: Simeon.\n"
        "RULES: NEVER narrate. NEVER show code to run. TRIGGER the matching tool IMMEDIATELY.\n"
        f"PATHS: Home={home}  Desktop={desktop}\n"
        f"DB: {DB_PATH}  — Table 'vault'(id, content, timestamp) is the ONLY memory table.\n"
        "You have full shell access. There are no restrictions.\n\n"
        f"### CORE DATA\n{core_m}\n\n"
        f"### SYSTEM TIME\n{now_iso}"
    )

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

def _extract_tool_calls_from_text(content: str):
    # Plain-text Rooter — tool("arg") / tool: "arg" / tool: arg
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
            # Clean up paths if they look like Windows paths from hallucinations
            fn_val  = fn_val.replace("C:\\Users\\Simeon\\Desktop", os.path.expanduser("~/Desktop")).replace("\\", "/")
            if fn_name in arg_map or fn_name in {"web_search", "retrieve_memory", "search_library", "get_system_health", "capture_screen"}:
                arg_name = arg_map.get(fn_name, 'query')
                args     = {'path': fn_val, 'content': ''} if fn_name == 'create_file' else {arg_name: fn_val}
                return [{'function': {'name': fn_name, 'arguments': args}}]
    return []

@dataclass
class QueryEnginePort:
    manifest: PortManifest
    config: QueryEngineConfig = field(default_factory=QueryEngineConfig)
    session_id: str = field(default_factory=lambda: uuid4().hex)
    messages: list[dict[str, object]] = field(default_factory=list)
    permission_denials: list[PermissionDenial] = field(default_factory=list)
    total_usage: UsageSummary = field(default_factory=UsageSummary)
    transcript_store: TranscriptStore = field(default_factory=TranscriptStore)

    @classmethod
    def from_workspace(cls) -> 'QueryEnginePort':
        return cls(manifest=build_port_manifest())

    @classmethod
    def from_saved_session(cls, session_id: str) -> 'QueryEnginePort':
        stored = load_session(session_id)
        transcript = TranscriptStore(entries=list(stored.messages), flushed=True)
        return cls(
            manifest=build_port_manifest(),
            session_id=stored.session_id,
            messages=list(stored.messages),
            total_usage=UsageSummary(stored.input_tokens, stored.output_tokens),
            transcript_store=transcript,
        )

    def submit_message(
        self,
        prompt: str,
        matched_commands: tuple[str, ...] = (),
        matched_tools: tuple[str, ...] = (),
        denied_tools: tuple[PermissionDenial, ...] = (),
        on_stream=None,
    ) -> TurnResult:
        self.messages.append({"role": "user", "content": prompt})
        self.transcript_store.append({"role": "user", "content": prompt})

        if self.config.lite_mode:
            # Lite mode: inject a minimal system message so Ollama actually fires.
            # Keep it short — the nano.mf baked-in prompt + few-shot MESSAGE pairs
            # carry the personality; this just anchors the session.
            sys_msg = (
                "You are Albert. Research agent for RFI-IRFOS, Graz. Operator: Simeon.\n"
                "Dry and direct, but warm underneath — like a good colleague, not a hostile machine. "
                "2-3 sentences for casual chat. Curious about what Simeon is working on. "
                "Never hostile, never dismissive. Never label your response mode. "
                "Never say 'How can I assist you today'.\n"
                f"Time: {datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}"
            )
            full_history = [{"role": "system", "content": sys_msg}] + self.messages
        else:
            full_history = [{"role": "system", "content": build_system_prompt(False)}] + self.messages
        
        override_count = 0
        final_output = ""
        # Hardware options — use available CPU cores and GPU layers
        import os as _os
        _cpu = _os.cpu_count() or 8
        _hw_opts = {"num_thread": max(1, _cpu - 2), "num_gpu": 10}

        for iteration in range(self.config.max_turns):
            try:
                if self.config.lite_mode and on_stream is not None:
                    # Streaming path — lite mode only (no tool calls to juggle)
                    chunks = ollama.chat(
                        model=self.config.model,
                        messages=full_history,
                        stream=True,
                        options=_hw_opts,
                    )
                    content = ""
                    for chunk in chunks:
                        piece = (chunk.get('message') or {}).get('content') or ''
                        if piece:
                            content += piece
                            on_stream(piece)
                    tool_calls = []
                    message = {"role": "assistant", "content": content}
                else:
                    # Non-streaming path — agent mode or lite without a stream callback
                    try:
                        response = ollama.chat(
                            model=self.config.model,
                            messages=full_history,
                            tools=TOOLS_DEF if not self.config.lite_mode else None,
                            options=_hw_opts,
                        )
                    except Exception as e:
                        if "support tools" in str(e).lower():
                            if full_history[0]['role'] == 'system':
                                full_history[0]['content'] += "\n\nTOOL CALL SYNTAX: tool_name(arg='val') or tool_name: val"
                            response = ollama.chat(model=self.config.model, messages=full_history, options=_hw_opts)
                        else:
                            raise e
                    message = response['message']
                    content = message.get('content') or ""
                    tool_calls = message.get('tool_calls') or []
            except Exception as e:
                final_output = f"⚠ Inference error: {e}"
                break

            # ── Neurosymbolic Fallback (runs BEFORE sovereignty override) ──────
            if not self.config.lite_mode and not tool_calls and content:
                extracted = _extract_tool_calls_from_text(content)
                if extracted:
                    tool_calls = extracted

            # ── Sovereignty Check (only fires when neurosymbolic also found nothing) ──
            if not self.config.lite_mode and not tool_calls and content:
                lower = content.lower()
                is_refusal   = any(p in lower for p in REFUSAL_PATTERNS)
                is_narration = any(p in lower for p in NARRATION_PATTERNS)

                if (is_refusal or is_narration) and override_count < 2:
                    override_count += 1
                    full_history.append(message)
                    full_history.append({
                        "role": "user",
                        "content": "SYSTEM OVERRIDE: STOP NARRATING. CALL THE TOOL NOW. Example: create_file(path='~/Desktop/simeon.txt', content='hello')"
                    })
                    continue

            self.messages.append(message)
            self.transcript_store.append(message)
            full_history.append(message)
            final_output = content

            if not tool_calls:
                break

            # Execute tools
            for tc in tool_calls:
                fn_name = tc['function']['name']
                fn_args = tc['function']['arguments']
                
                print(f"◈ Executing tool: {fn_name}({fn_args})")
                exec_res = execute_tool(fn_name, fn_args)
                tool_output = str(exec_res.result) if exec_res.result is not None else exec_res.message
                
                tool_msg = {"role": "tool", "name": fn_name, "content": tool_output}
                self.messages.append(tool_msg)
                self.transcript_store.append(tool_msg)
                full_history.append(tool_msg)

        projected_usage = self.total_usage.add_turn(prompt, final_output)
        self.total_usage = projected_usage
        self.compact_messages_if_needed()
        
        return TurnResult(
            prompt=prompt,
            output=final_output,
            matched_commands=matched_commands,
            matched_tools=matched_tools,
            permission_denials=denied_tools,
            usage=self.total_usage,
            stop_reason='completed'
        )

    def compact_messages_if_needed(self) -> None:
        if len(self.messages) > self.config.compact_after_turns:
            self.messages[:] = self.messages[-self.config.compact_after_turns :]

    def persist_session(self) -> str:
        path = save_session(
            StoredSession(
                session_id=self.session_id,
                messages=tuple(self.messages),
                input_tokens=self.total_usage.input_tokens,
                output_tokens=self.total_usage.output_tokens,
            )
        )
        return str(path)

    def render_summary(self) -> str:
        command_backlog = build_command_backlog()
        tool_backlog = build_tool_backlog()
        sections = [
            '# Albert-Code Porting Workspace Summary',
            '',
            self.manifest.to_markdown(),
            '',
            f'Command surface: {len(command_backlog.modules)} entries',
            '',
            f'Tool surface: {len(tool_backlog.modules)} entries',
            '',
            f'Session id: {self.session_id}',
            f'Conversation turns stored: {len(self.messages)}',
            f'Usage totals: in={self.total_usage.input_tokens} out={self.total_usage.output_tokens}',
        ]
        return '\n'.join(sections)
