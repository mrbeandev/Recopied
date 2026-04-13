# Changelog

All notable changes to Recopied will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] - 2026-04-14

### Added

- **Copy notifications** — System notification shown whenever content is copied to clipboard. Configurable in Settings with enable/disable toggle, option to show or hide copied content in the notification body, and a test notification button.
- **Flatpak support** — Recopied is now available as a Flatpak package via a self-hosted repository. Install with `flatpak remote-add` + `flatpak install`.

## [1.1.0] - 2026-03-22

### Added

- **Configurable clipboard limit** — Set the maximum number of items to keep in history from the Settings panel (default: 20, no upper cap). Oldest non-pinned items are automatically pruned when the limit is reached. Pinned items are never deleted.
- Warning banner in Settings when limit is set above 20 ("increase at your own risk").

### Fixed

- **Clipboard watcher hang** — `xclip`/`wl-paste` subprocesses now run with a 1.5-second timeout and are killed if they stall. This was the root cause of the app freezing and stopping to save new items over time.
- **SQLite WAL mode** — Database now opens in WAL journal mode with `synchronous=NORMAL`, preventing read/write lock contention between the background watcher thread and the UI commands.

## [1.0.0] - 2025-07-16

### Added

- Clipboard history with text and image support
- Global hotkey to toggle popup (default: `Ctrl+Shift+V`)
- Configurable keyboard shortcut via Settings panel
- Pin important items to keep them at the top
- Search/filter clipboard history
- Click-to-copy with auto-hide
- Fullscreen / windowed toggle
- Keyboard navigation (↑ / ↓ / Enter)
- System tray icon with show/quit menu
- Win11-inspired dark UI theme
- Frameless, always-on-top popup window
- Drag handle for repositioning
- SQLite-backed persistent storage
- SHA-256 dedup to avoid duplicate entries
- Auto-prune at 500 items
- Image preview with asset protocol
- Slide-up open animation
- Professional Lucide icon set throughout
