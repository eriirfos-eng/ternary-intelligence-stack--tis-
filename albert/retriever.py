import sys
import json

VAULT_PATH = "albert_vault.jsonl"

def retrieve_memory(query):
    query = query.lower()
    results = []
    
    try:
        with open(VAULT_PATH, "r", encoding="utf-8") as f:
            for line in f:
                if query in line.lower():
                    entry = json.loads(line.strip())
                    results.append(f"[{entry['timestamp']}] {entry['content']}")
        
        if results:
            return "Data retrieved:\n" + "\n".join(results)
        else:
            return f"0 Static: No memories found matching '{query}'."
            
    except FileNotFoundError:
        return "0 Static: Vault is empty or does not exist yet."
    except Exception as e:
        return f"-1 Entropy. Failed to access vault: {str(e)}"

if __name__ == "__main__":
    if len(sys.argv) > 1:
        search_term = " ".join(sys.argv[1:])
        print(retrieve_memory(search_term))
    else:
        print("0 Static: No search query provided.")
