# Eyeclipse

A lightweight Linux (X11) desktop utility that lets you select a screen region, extract text via OCR, translate it, and display the result in a floating overlay. Supports one-shot and real-time live monitoring modes.

## Features

- **Global hotkey** (`Super+Shift+S`) — works system-wide, even when minimized
- **Region selection** — fullscreen dimmed overlay with crosshair and drag-to-select
- **OCR** — extracts text from the selected region via Tesseract
- **Translation** — pluggable API backends: DeepL, LibreTranslate, OpenAI
- **Result overlay** — floating borderless window near the selection, dismissed with Escape or click outside
- **System tray** — background icon with right-click menu (toggle mode, quit)
- **Live mode** — continuously monitors a region for text changes, only re-processes when content changes (pixel diff)
- **Configurable** — languages, API backend, hotkey, refresh interval via config file

## System Requirements

- Linux with X11 display server
- A compositor (Picom, Compton, etc.) for transparency effects
- Rust toolchain (1.75+)

## Installation

### 1. Install system dependencies

```bash
sudo apt-get install -y \
  libxdo-dev \
  libxcb-shm0-dev \
  libxcb-randr0-dev \
  libxcb-xfixes0-dev \
  libxcb-composite0-dev \
  libxcb-render0-dev \
  clang \
  libtesseract-dev \
  libleptonica-dev \
  tesseract-ocr \
  tesseract-ocr-eng \
  tesseract-ocr-jpn \
  libglib2.0-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev
```

<details>
<summary>To uninstall these dependencies later</summary>

```bash
sudo apt-get remove --purge -y \
  libxdo-dev \
  libxcb-shm0-dev \
  libxcb-randr0-dev \
  libxcb-xfixes0-dev \
  libxcb-composite0-dev \
  libxcb-render0-dev \
  clang \
  libtesseract-dev \
  libleptonica-dev \
  tesseract-ocr \
  tesseract-ocr-eng \
  tesseract-ocr-jpn \
  libglib2.0-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  && sudo apt-get autoremove -y
```
</details>

### 2. Install additional OCR language packs (optional)

```bash
# Chinese (Simplified)
sudo apt-get install -y tesseract-ocr-chi-sim

# Korean
sudo apt-get install -y tesseract-ocr-kor

# German
sudo apt-get install -y tesseract-ocr-deu
```

### 3. Build

```bash
# Full build (all features)
cargo build --release

# Minimal build (no tray icon, no OCR — for testing UI only)
cargo build --release --no-default-features

# OCR only (no system tray)
cargo build --release --no-default-features --features ocr

# Tray only (no OCR)
cargo build --release --no-default-features --features tray
```

The binary will be at `target/release/eyeclipse`.

### 4. Install the binary (optional)

```bash
# Copy to a directory on your PATH
sudo cp target/release/eyeclipse /usr/local/bin/
```

## Configuration

On first run, a default config file is created at:

```
~/.config/eyeclipse/config.toml
```

### Example config

```toml
# Keyboard shortcut to trigger capture (currently hardcoded — this is for future use)
hotkey = "Super+Shift+S"

# Source language code (for translation API)
source_lang = "ja"

# Target language code (for translation API)
target_lang = "en"

# Translation backend: "deepl", "libretranslate", or "openai"
api_backend = "deepl"

# Your API key
api_key = "your-deepl-api-key-here"

# Custom API URL (leave empty for defaults)
# DeepL Free: https://api-free.deepl.com
# DeepL Pro: https://api.deepl.com
# LibreTranslate: http://localhost:5000
# OpenAI: https://api.openai.com
api_url = ""

# Mode: "oneshot" or "live"
mode = "oneshot"

# Live mode refresh interval in milliseconds
live_interval_ms = 1000

# Tesseract OCR language(s) — use '+' to combine
# Common: eng, jpn, chi_sim, kor, deu, fra, spa
ocr_lang = "jpn+eng"
```

### API Backend Setup

#### DeepL (recommended)

1. Sign up at [deepl.com/pro](https://www.deepl.com/pro) (free tier: 500,000 chars/month)
2. Get your API key from the account page
3. Set in config:
   ```toml
   api_backend = "deepl"
   api_key = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx:fx"
   ```

#### LibreTranslate (self-hosted, private)

1. Run LibreTranslate locally:
   ```bash
   docker run -d -p 5000:5000 libretranslate/libretranslate
   ```
2. Set in config:
   ```toml
   api_backend = "libretranslate"
   api_url = "http://localhost:5000"
   api_key = ""
   ```

#### OpenAI

1. Get an API key from [platform.openai.com](https://platform.openai.com)
2. Set in config:
   ```toml
   api_backend = "openai"
   api_key = "sk-..."
   ```

## Usage

### Start the app

```bash
eyeclipse
```

Or with debug logging:

```bash
RUST_LOG=debug eyeclipse
```

### One-shot translation

1. Press `Super+Shift+S`
2. Screen dims — click and drag to select a region containing text
3. Release mouse — OCR runs and text is translated
4. A floating overlay appears near the selection with source + translated text
5. Press `Escape` or click outside to dismiss

### Live translation mode

1. Change mode in config: `mode = "live"` (or toggle via tray menu)
2. Press `Super+Shift+S` and select a region
3. The overlay stays open and updates whenever the text in the region changes
4. Click "Stop Live" or press `Escape` to stop monitoring

### System tray

- Right-click the tray icon to:
  - **Capture Region** — trigger capture manually
  - **Toggle Live Mode** — switch between one-shot and live
  - **Quit** — exit the app

## Project Structure

```
src/
├── main.rs           # Entry point, event loop
├── lib.rs            # Library exports for tests
├── config.rs         # AppConfig, TOML load/save
├── hotkey.rs         # Global hotkey (Super+Shift+S)
├── selector.rs       # X11 region selection overlay
├── capture.rs        # Screen capture (xcap)
├── ocr.rs            # Tesseract OCR (leptess)
├── translate/
│   ├── mod.rs        # TranslationBackend trait
│   ├── deepl.rs      # DeepL API
│   ├── libretranslate.rs  # LibreTranslate API
│   └── openai.rs     # OpenAI API
├── overlay.rs        # egui floating result window
├── tray.rs           # System tray (GTK)
├── live.rs           # Live monitoring loop
└── diff.rs           # Image change detection
tests/
├── config_test.rs    # Config serialization tests
├── diff_test.rs      # Image diff logic tests
└── translate_test.rs # Translation backend mock tests
```

## Running Tests

```bash
cargo test
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `ocr`   | Yes     | Tesseract OCR support (requires `libtesseract-dev`, `libleptonica-dev`) |
| `tray`  | Yes     | System tray icon (requires `libgtk-3-dev`, `libayatana-appindicator3-dev`) |

## Troubleshooting

### "Failed to init Tesseract"

Install the OCR engine and language packs:
```bash
sudo apt-get install tesseract-ocr tesseract-ocr-eng tesseract-ocr-jpn
```

### Transparent overlay shows black background

You need a compositor running:
```bash
# Install picom
sudo apt-get install picom

# Start it
picom --daemon
```

### Hotkey doesn't work

- Make sure no other app is grabbing `Super+Shift+S`
- The app must be running (check the tray icon)
- X11 is required — Wayland is not supported

### "No API key set" warning

Edit `~/.config/eyeclipse/config.toml` and add your API key.

## License

MIT
