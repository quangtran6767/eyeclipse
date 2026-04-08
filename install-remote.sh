#!/usr/bin/env bash
set -euo pipefail

# Eyeclipse — remote installer
# Downloads a prebuilt binary from GitHub Releases, or falls back to building from source.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/quangtran6767/eyeclipse/main/install-remote.sh | sh

REPO="quangtran6767/eyeclipse"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[✗]${NC} $*"; exit 1; }

# --- Detect platform ---
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_TAG="linux" ;;
    Darwin) OS_TAG="macos" ;;
    *)      error "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
    x86_64)         ARCH_TAG="x86_64" ;;
    aarch64|arm64)  ARCH_TAG="aarch64" ;;
    *)              error "Unsupported architecture: $ARCH" ;;
esac

ASSET="eyeclipse-${OS_TAG}-${ARCH_TAG}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

info "Detected platform: ${OS_TAG}-${ARCH_TAG}"

# --- Install system dependencies ---
install_deps() {
    case "$OS" in
        Linux)
            info "Installing system dependencies..."
            if command -v apt-get &>/dev/null; then
                sudo apt-get update -qq
                sudo apt-get install -y --no-install-recommends \
                    tesseract-ocr tesseract-ocr-eng tesseract-ocr-jpn \
                    libgtk-3-0 libappindicator3-1 \
                    slop \
                    fonts-noto-cjk fonts-noto 2>/dev/null || true
            elif command -v dnf &>/dev/null; then
                sudo dnf install -y \
                    tesseract tesseract-langpack-eng tesseract-langpack-jpn \
                    gtk3 libappindicator-gtk3 \
                    slop \
                    google-noto-sans-cjk-fonts 2>/dev/null || true
            elif command -v pacman &>/dev/null; then
                sudo pacman -Syu --noconfirm --needed \
                    tesseract tesseract-data-eng tesseract-data-jpn \
                    gtk3 libappindicator-gtk3 \
                    slop \
                    noto-fonts noto-fonts-cjk 2>/dev/null || true
            else
                warn "Could not detect package manager. Install Tesseract and slop manually."
            fi
            ;;
        Darwin)
            if command -v brew &>/dev/null; then
                info "Installing Tesseract via Homebrew..."
                brew install tesseract 2>/dev/null || true
            else
                warn "Homebrew not found. Install Tesseract manually: https://github.com/tesseract-ocr/tesseract"
            fi
            ;;
    esac
}

# --- Try downloading prebuilt binary ---
download_binary() {
    info "Downloading ${ASSET}..."
    local tmp_dir
    tmp_dir="$(mktemp -d)"
    local tmp_file="${tmp_dir}/${ASSET}"

    if curl -fsSL -o "$tmp_file" "$DOWNLOAD_URL" 2>/dev/null; then
        info "Extracting..."
        tar -xzf "$tmp_file" -C "$tmp_dir"

        info "Installing to ${INSTALL_DIR}/eyeclipse..."
        if [ "$OS" = "Darwin" ] && [ "$INSTALL_DIR" = "/usr/local/bin" ]; then
            install -m 755 "${tmp_dir}/eyeclipse" "${INSTALL_DIR}/eyeclipse" 2>/dev/null \
                || sudo install -m 755 "${tmp_dir}/eyeclipse" "${INSTALL_DIR}/eyeclipse"
        else
            sudo install -m 755 "${tmp_dir}/eyeclipse" "${INSTALL_DIR}/eyeclipse"
        fi

        rm -rf "$tmp_dir"
        return 0
    else
        rm -rf "$tmp_dir"
        return 1
    fi
}

# --- Build from source (fallback) ---
build_from_source() {
    warn "No prebuilt binary found for ${OS_TAG}-${ARCH_TAG}. Building from source..."

    # Check for Rust
    if ! command -v cargo &>/dev/null; then
        info "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi

    local tmp_dir
    tmp_dir="$(mktemp -d)"
    info "Cloning repository..."
    git clone --depth 1 "https://github.com/${REPO}.git" "$tmp_dir/eyeclipse"
    cd "$tmp_dir/eyeclipse"

    info "Building (this may take a few minutes)..."
    cargo build --release

    info "Installing to ${INSTALL_DIR}/eyeclipse..."
    if [ "$OS" = "Darwin" ] && [ "$INSTALL_DIR" = "/usr/local/bin" ]; then
        install -m 755 target/release/eyeclipse "${INSTALL_DIR}/eyeclipse" 2>/dev/null \
            || sudo install -m 755 target/release/eyeclipse "${INSTALL_DIR}/eyeclipse"
    else
        sudo install -m 755 target/release/eyeclipse "${INSTALL_DIR}/eyeclipse"
    fi

    rm -rf "$tmp_dir"
}

# --- Create default config ---
create_config() {
    local config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/eyeclipse"
    if [ ! -f "$config_dir/config.toml" ]; then
        info "Creating default config at $config_dir/config.toml"
        mkdir -p "$config_dir"
        cat > "$config_dir/config.toml" << 'EOF'
hotkey = "super+shift+s"
source_lang = "ja"
target_lang = "en"
api_backend = "deepl"
api_key = ""
api_url = ""
mode = "oneshot"
live_interval_ms = 1000
ocr_lang = "jpn+eng"
settle_time_ms = 1500
live_timing = "settle"
EOF
        warn "Edit $config_dir/config.toml to set your API key and preferences."
    fi
}

# --- Main ---
install_deps

if download_binary; then
    info "Prebuilt binary installed successfully!"
else
    build_from_source
fi

create_config

echo ""
info "Eyeclipse installed successfully!"
echo "  Binary:  ${INSTALL_DIR}/eyeclipse"
echo "  Config:  ${XDG_CONFIG_HOME:-$HOME/.config}/eyeclipse/config.toml"
echo ""
echo "  Run:     eyeclipse"
echo "  Hotkey:  Super+Shift+S (default)"
echo ""
if [ "$OS" = "Darwin" ]; then
    warn "On macOS, grant Screen Recording and Accessibility permissions in System Settings."
fi
