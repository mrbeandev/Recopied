#!/usr/bin/env bash
#
# Recopied — local Flatpak build + publish script
# Usage: ./scripts/flatpak-publish.sh [version]
# Example: ./scripts/flatpak-publish.sh 1.1.0  (default: reads from package.json)
#
# What this does:
#   1. Checks / installs required tools (flatpak, flatpak-builder)
#   2. Installs GNOME Platform 48 runtime if needed
#   3. Builds the Flatpak bundle using the manifest (no xvfb, no CI container)
#   4. Exports the bundle into a local OSTree repo
#   5. Pushes the static repo to the 'gh-pages' branch → GitHub Pages serves it
#   6. Uploads recopied.flatpak to the GitHub release
#
# Requirements:
#   - flatpak          (sudo apt-get install flatpak)
#   - flatpak-builder  (sudo apt-get install flatpak-builder)
#   - gh (GitHub CLI)  (https://cli.github.com — run 'gh auth login' first)
#   - GNOME Platform 48 runtime (this script installs it if missing)
#
# Users can then install Recopied with:
#   flatpak remote-add --if-not-exists recopied https://mrbeandev.github.io/Recopied/
#   flatpak install recopied com.recopied.app

set -euo pipefail

# ── helpers ──────────────────────────────────────────────────────────────────
bold() { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '  \033[34m→\033[0m %s\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
die()  { printf '\033[31mError:\033[0m %s\n' "$*" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$SCRIPT_DIR")"
cd "$ROOT"

# ── version ──────────────────────────────────────────────────────────────────
if [ $# -ge 1 ]; then
  VERSION="$1"
else
  VERSION=$(grep -oP '"version":\s*"\K[^"]+' package.json | head -1)
fi
TAG="v$VERSION"
bold "Building Flatpak for Recopied $VERSION ($TAG)"
echo ""

# ── dependencies ─────────────────────────────────────────────────────────────
command -v flatpak >/dev/null 2>&1        || die "flatpak not found. Install: sudo apt-get install flatpak"
command -v flatpak-builder >/dev/null 2>&1 || die "flatpak-builder not found. Install: sudo apt-get install flatpak-builder"
command -v gh >/dev/null 2>&1             || die "gh (GitHub CLI) not found. Install: https://cli.github.com"

# ── pre-built binary must already exist ──────────────────────────────────────
BINARY="src-tauri/target/release/recopied"
[ -f "$BINARY" ] || die "Release binary not found at $BINARY.
  Run first:  PKG_CONFIG_PATH=/tmp/pkgconfig cargo tauri build  (or: bash scripts/release.sh $VERSION)"

info "Using binary: $BINARY ($(wc -c < "$BINARY" | awk '{printf "%.1f MB", $1/1024/1024}'))"

# Copy binary into flatpak/ dir so the manifest 'type: file' source finds it
cp "$BINARY" flatpak/recopied
chmod +x flatpak/recopied
ok "Binary copied to flatpak/recopied"

# ── GNOME Platform runtime ────────────────────────────────────────────────────
info "Checking GNOME Platform 48 runtime..."
if ! flatpak list --runtime --columns=application,branch 2>/dev/null | grep -q "org.gnome.Platform.*48"; then
  info "Installing GNOME Platform 48 (this may take a few minutes — ~700 MB)..."
  flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  flatpak install --user --noninteractive flathub org.gnome.Platform//48 org.gnome.Sdk//48
  ok "GNOME Platform 48 installed"
else
  ok "GNOME Platform 48 already installed"
fi

# ── build ─────────────────────────────────────────────────────────────────────
BUILD_DIR="$ROOT/flatpak-build-dir"
REPO_DIR="$ROOT/flatpak-repo"

bold ""
bold "Building Flatpak bundle..."
flatpak-builder \
  --user \
  --install-deps-from=flathub \
  --force-clean \
  --disable-tests \
  --repo="$REPO_DIR" \
  "$BUILD_DIR" \
  flatpak/com.recopied.app.yml

ok "flatpak-builder succeeded"

# ── bundle ────────────────────────────────────────────────────────────────────
info "Creating .flatpak bundle..."
flatpak build-bundle "$REPO_DIR" recopied.flatpak com.recopied.app stable
ok "Bundle created: recopied.flatpak ($(du -h recopied.flatpak | cut -f1))"

# ── update static repo metadata ──────────────────────────────────────────────
info "Updating OSTree repo metadata..."
flatpak build-update-repo \
  --generate-static-deltas \
  --default-branch=stable \
  "$REPO_DIR"
ok "Repo metadata updated"

# ── add index.html ────────────────────────────────────────────────────────────
cat > "$REPO_DIR/index.html" << 'HTML'
<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>Recopied Flatpak Repository</title>
<style>body{font-family:sans-serif;max-width:700px;margin:3rem auto;padding:0 1rem}code{background:#f4f4f4;padding:.2em .4em;border-radius:4px}</style>
</head>
<body>
<h1>📋 Recopied — Flatpak Repository</h1>
<p>This is the official self-hosted Flatpak repository for <strong>Recopied</strong>,
   a Linux clipboard manager with a Win+V shortcut.</p>
<h2>Add this repository</h2>
<pre><code>flatpak remote-add --if-not-exists recopied https://mrbeandev.github.io/Recopied/</code></pre>
<h2>Install Recopied</h2>
<pre><code>flatpak install recopied com.recopied.app</code></pre>
<h2>Or install directly from the .flatpak bundle</h2>
<p>Download <code>recopied.flatpak</code> from the
   <a href="https://github.com/mrbeandev/Recopied/releases">GitHub Releases</a> page, then:</p>
<pre><code>flatpak install recopied.flatpak</code></pre>
<hr>
<p><a href="https://github.com/mrbeandev/Recopied">GitHub</a> ·
   <a href="https://github.com/mrbeandev/Recopied/releases">Releases</a></p>
</body></html>
HTML

# ── push repo to gh-pages branch ─────────────────────────────────────────────
bold ""
bold "Pushing static repo to gh-pages branch..."

DEPLOY_DIR="$(mktemp -d)"
git clone --depth=1 --branch gh-pages "$(git remote get-url origin)" "$DEPLOY_DIR" 2>/dev/null || {
  # Branch doesn't exist yet — clone repo and create orphan branch
  git clone --depth=1 "$(git remote get-url origin)" "$DEPLOY_DIR"
  cd "$DEPLOY_DIR"
  git checkout --orphan gh-pages
  git rm -rf . 2>/dev/null || true
  cd "$ROOT"
}

info "Copying OSTree repo files..."
cp -a "$REPO_DIR/". "$DEPLOY_DIR/"

cd "$DEPLOY_DIR"
git config user.email "actions@github.com"
git config user.name "Recopied Release Bot"
git add -A
git commit -m "chore: publish Flatpak repo for $TAG" || { info "Nothing changed in repo"; }
git push origin gh-pages
cd "$ROOT"
rm -rf "$DEPLOY_DIR"
ok "Pushed to gh-pages"

# ── upload bundle to GitHub release ──────────────────────────────────────────
bold ""
info "Uploading recopied.flatpak to GitHub release $TAG..."
gh release upload "$TAG" recopied.flatpak \
  --repo mrbeandev/Recopied \
  --clobber
ok "Bundle uploaded to $TAG release"

# ── cleanup ───────────────────────────────────────────────────────────────────
rm -f flatpak/recopied  # remove the temp copy of the binary from flatpak/

bold ""
bold "✓ Flatpak published successfully!"
echo ""
echo "  Install command:"
echo "    flatpak remote-add --if-not-exists recopied https://mrbeandev.github.io/Recopied/"
echo "    flatpak install recopied com.recopied.app"
echo ""
echo "  Or install from bundle:"
echo "    flatpak install recopied.flatpak"
