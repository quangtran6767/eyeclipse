#!/usr/bin/env bash
set -euo pipefail

# Eyeclipse — cross-platform install script
# Installs system dependencies, builds from source, and installs the binary.

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[✗]${NC} $*"; exit 1; }

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OS="$(uname -s)"

# --- Install system dependencies ---
case "$OS" in
    Linux)
        # Detect package manager
        if command -v apt-get &>/dev/null; then
            PM=apt
        elif command -v dnf &>/dev/null; then
            PM=dnf
        elif command -v pacman &>/dev/null; then
            PM=pacman
        else
            error "Unsupported package manager. Install dependencies manually (see README)."
        fi

        info "Installing system dependencies ($PM)..."

        case $PM in
            apt)
                sudo apt-get update -qq
                sudo apt-get install -y --no-install-recommends \
                    build-essential pkg-config \
                    libtesseract-dev libleptonica-dev tesseract-ocr \
                    libgtk-3-dev libappindicator3-dev \
                    libxcb1-dev libxcb-randr0-dev libxcb-shm0-dev libxcb-xfixes0-dev \
                    slop \
                    fonts-noto-cjk fonts-noto
                ;;
            dnf)
                sudo dnf install -y \
                    gcc gcc-c++ pkg-config \
                    tesseract-devel leptonica-devel tesseract \
                    gtk3-devel libappindicator-gtk3-devel \
                    libxcb-devel \
                    slop \
                    google-noto-sans-cjk-fonts google-noto-sans-fonts
                ;;
            pacman)
                sudo pacman -Syu --noconfirm --needed \
                    base-devel pkg-config \
                    tesseract leptonica \
                    gtk3 libappindicator-gtk3 \
                    libxcb \
                    slop \
                    noto-fonts noto-fonts-cjk
                ;;
        esac
        ;;
    Darwin)
        info "Installing system dependencies via Homebrew..."
        if ! command -v brew &>/dev/null; then
            error "Homebrew is required on macOS. Install it from https://brew.sh"
        fi
        brew install tesseract
        # Install language packs
        info "Tesseract languages are bundled with Homebrew tesseract."
        info "For additional languages, set TESSDATA_PREFIX and download .traineddata files."
        ;;
    *)
        error "Unsupported OS: $OS"
        ;;
esac

# --- Install Rust if needed ---
if ! command -v cargo &>/dev/null; then
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

# --- Install Tesseract language data (common) ---
info "Checking Tesseract languages..."
for lang in eng jpn vie; do
    if ! tesseract --list-langs 2>/dev/null | grep -q "^${lang}$"; then
        warn "Tesseract language '$lang' not found. Install it with:"
        case "$OS" in
            Linux)
                echo "  sudo apt install tesseract-ocr-$lang   # Debian/Ubuntu"
                echo "  sudo dnf install tesseract-langpack-$lang  # Fedora"
                echo "  sudo pacman -S tesseract-data-$lang    # Arch"
                ;;
            Darwin)
                echo "  Languages are bundled with 'brew install tesseract'."
                echo "  For extra languages, download .traineddata files to \$(brew --prefix)/share/tessdata/"
                ;;
        esac
    fi
done

# --- Build ---
info "Building eyeclipse (release)..."
cd "$SCRIPT_DIR"
cargo build --release

# --- Install binary ---
info "Installing to $INSTALL_DIR/eyeclipse..."
if [ "$OS" = "Darwin" ] && [ "$INSTALL_DIR" = "/usr/local/bin" ]; then
    # On macOS, /usr/local/bin is usually writable without sudo
    install -m 755 target/release/eyeclipse "$INSTALL_DIR/eyeclipse" 2>/dev/null \
        || sudo install -m 755 target/release/eyeclipse "$INSTALL_DIR/eyeclipse"
else
    sudo install -m 755 target/release/eyeclipse "$INSTALL_DIR/eyeclipse"
fi

# --- Create default config if not exists ---
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/eyeclipse"
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    info "Creating default config at $CONFIG_DIR/config.toml"
    mkdir -p "$CONFIG_DIR"
    cat > "$CONFIG_DIR/config.toml" << 'EOF'
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
    warn "Edit $CONFIG_DIR/config.toml to set your API key and preferences."
fi

# --- Done ---
echo ""
info "Eyeclipse installed successfully!"
echo "  Binary:  $INSTALL_DIR/eyeclipse"
echo "  Config:  $CONFIG_DIR/config.toml"
echo ""
echo "  Run:     eyeclipse"
echo "  Hotkey:  Super+Shift+S (default)"
echo ""
