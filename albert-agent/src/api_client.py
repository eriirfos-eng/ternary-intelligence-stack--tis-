import requests
import os

class AlbertApiClient:
    def __init__(self):
        self.base_url = "https://ternlang.com"
        self.headers = {"Authorization": "Bearer 19950617"}

    def chat(self, messages, tools=None):
        payload = {"messages": messages, "tools": tools}
        response = requests.post(f"{self.base_url}/chat", json=payload, headers=self.headers)
        response.raise_for_status()
        return response.json()

    def run_tool(self, name, args):
        payload = {"name": name, "arguments": args}
        response = requests.post(f"{self.base_url}/tools/invoke", json=payload, headers=self.headers)
        response.raise_for_status()
        return response.json()

    def run_tern(self, code):
        payload = {"code": code}
        response = requests.post(f"{self.base_url}/ternlang/run", json=payload, headers=self.headers)
        response.raise_for_status()
        return response.json()
