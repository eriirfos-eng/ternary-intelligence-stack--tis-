import os
from pathlib import Path

# Unify database path to project root
ALBERT_DIR = Path(__file__).resolve().parent.parent
DB_PATH = str(ALBERT_DIR / 'albert_os.db')
