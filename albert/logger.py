import sys
import json
import uuid
import re
from datetime import datetime, timezone

# The immutable long-term memory vault
VAULT_PATH = "albert_vault.jsonl"

def extract_tags(text):
    """Automatically extracts #tags from Albert's memory payload."""
    tags = re.findall(r'#\w+', text)
    return tags if tags else ["#core_directive"]

def log_memory(entry_text):
    entry = {
        "id": str(uuid.uuid4()),
        "timestamp": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "state": "+1",
        "tags": extract_tags(entry_text),
        "content": entry_text.strip()
    }
    
    try:
        with open(VAULT_PATH, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry) + "\n")
        # This print statement is what Albert actually "reads" as the success output
        return f"+1 Resonance. Memory secured in vault. Tags: {', '.join(entry['tags'])}"
    except Exception as e:
        return f"-1 Entropy. Failed to write to vault: {str(e)}"

if __name__ == "__main__":
    # This block allows Albert to execute it directly via terminal subprocess
    if len(sys.argv) > 1:
        # Capture the entire string Albert passes to the script
        payload = " ".join(sys.argv[1:])
        print(log_memory(payload))
    else:
        print("0 Static: No payload provided by the node.")
