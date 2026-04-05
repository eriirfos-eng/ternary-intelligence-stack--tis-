#!/bin/bash
set -e

# Ternlang Quickstart Installer
# "The triadic future starts with a single command."

BLUE='\033[0;34m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== TERNARY INTELLIGENCE STACK (TIS) INSTALLER ===${NC}"
echo "----------------------------------------------------"

# 1. Dependency Check
if ! command -v curl &> /dev/null; then
    echo "Installing curl..."
    sudo apt-get update && sudo apt-get install -y curl
fi

if ! command -v rustc &> /dev/null; then
    echo "Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
fi

# 2. Workspace Setup
INSTALL_DIR="$HOME/.ternlang"
echo -e "Setting up Ternlang in ${GREEN}${INSTALL_DIR}${NC}..."

if [ -d "$INSTALL_DIR" ]; then
    echo "Existing installation found. Updating..."
    cd "$INSTALL_DIR"
    git pull origin main
else
    git clone https://github.com/eriirfos-eng/ternary-intelligence-stack.git "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi

# 3. Build Core Tooling
echo -e "${BLUE}Building Ternlang CLI, Translator, and Auditor...${NC}"
cargo build --release --workspace

# 4. PATH Integration
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

# Link binaries
cp target/release/ternlang-cli "$BIN_DIR/tern"
cp target/release/ternlang-translator "$BIN_DIR/tern-trans"
cp target/release/ternlang-audit "$BIN_DIR/tern-audit"

echo -e "${GREEN}SUCCESS: Ternlang tools installed to $BIN_DIR${NC}"
echo "----------------------------------------------------"
echo -e "Try running: ${BLUE}tern --help${NC}"
echo -e "Quickstart manual: ${BLUE}https://ternlang.com/QUICKSTART.html${NC}"
echo "----------------------------------------------------"
echo "Note: You may need to restart your terminal or run 'export PATH=\$PATH:\$HOME/.local/bin'"
