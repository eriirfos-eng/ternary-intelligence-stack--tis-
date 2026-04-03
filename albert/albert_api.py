#!/usr/bin/env python3
from flask import Flask, request, jsonify
from llama_cpp import Llama
from datetime import datetime, timezone
import os
import json

# === Config ===
MODEL_PATH = os.path.expanduser("~/models/mistral-7b-instruct-v0.2.Q4_K_M.gguf")
N_THREADS = os.cpu_count() or 8   # auto detect CPU cores
N_CTX = 4096                      # safe context size (bump if RAM allows)

# === Load model ===
print(f"[{datetime.now(timezone.utc).isoformat()}] Loading Mistral model from {MODEL_PATH}")
# NOTE: n_gpu_layers=-1 will offload all layers to GPU if NVIDIA drivers are working.
# If nvidia-smi fails, this will fall back to CPU.
llm = Llama(
    model_path=MODEL_PATH,
    n_threads=N_THREADS,
    n_ctx=N_CTX,
    n_gpu_layers=-1,      # Use GPU if available
    use_mlock=True,       # lock model in RAM
    embedding=False,      # CPU inference only
    verbose=False
)

app = Flask("AlbertAPI")

def log_entry(prompt, response):
    entry = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "prompt": prompt,
        "response": response
    }
    with open("albert_log.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(entry) + "\n")

@app.route("/ask", methods=["POST"])
def ask():
    data = request.get_json(force=True)
    prompt = data.get("prompt", "")
    max_tokens = data.get("max_tokens", 256)

    if not prompt.strip():
        return jsonify({"error": "Empty prompt"}), 400

    try:
        output = llm(prompt, max_tokens=max_tokens, stop=["</s>", "User:"])
        response = output["choices"][0]["text"].strip()
        log_entry(prompt, response)
        return jsonify({
            "prompt": prompt,
            "response": response,
            "tokens_used": output["usage"]
        })
    except Exception as e:
        return jsonify({"error": str(e)}), 500

@app.route("/health", methods=["GET"])
def health():
    return jsonify({"status": "ok", "model": os.path.basename(MODEL_PATH)})

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)

