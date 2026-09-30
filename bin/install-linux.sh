#!/bin/bash
# Install a Linux build without exposing agent wrappers outside Wezmux.
set -euo pipefail

if [ "$(uname -s)" != Linux ]; then
    echo "This installer requires Linux." >&2
    exit 1
fi

prefix="${1:?Usage: install-linux.sh PREFIX BUILD_DIR}"
build_dir="${2:?Usage: install-linux.sh PREFIX BUILD_DIR}"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$prefix"
prefix="$(cd "$prefix" && pwd)"
app_dir="$prefix/lib/wezmux"

# Validate all inputs before replacing any installed binary.
for binary in wezterm wezterm-gui wezterm-mux-server strip-ansi-escapes; do
    if [ ! -x "$build_dir/$binary" ]; then
        echo "Missing executable: $build_dir/$binary" >&2
        exit 1
    fi
done

mkdir -p "$app_dir" "$prefix/bin" "$prefix/share/applications" "$prefix/share/icons/hicolor/256x256/apps"
for binary in wezterm wezterm-gui wezterm-mux-server strip-ansi-escapes; do
    install -m 755 "$build_dir/$binary" "$app_dir/$binary"
done
# current_exe resolves the GUI symlink to this directory; its sibling bin/
# is discovered by the existing Wezmux wrapper lookup on startup.
mkdir -p "$app_dir/bin"
cp -R "$repo_dir/bin/." "$app_dir/bin/"
chmod +x "$app_dir/bin/claude" "$app_dir/bin/omp" "$app_dir/bin/"*.sh "$app_dir/bin/hooks/"*.sh "$app_dir/bin/hooks/codex/"*.sh
ln -sfn "$app_dir/wezterm-gui" "$prefix/bin/wezmux"
ln -sfn "$app_dir/wezterm" "$prefix/bin/wezmux-cli"
install -m 644 "$repo_dir/assets/wezmux-logo.png" "$prefix/share/icons/hicolor/256x256/apps/wezmux.png"

# Desktop Exec paths are quoted and escaped according to desktop-entry rules.
exec_path="$app_dir/wezterm-gui"
exec_path="${exec_path//\\/\\\\}"
exec_path="${exec_path//\"/\\\"}"
exec_path="${exec_path//\$/\\\$}"
exec_path="${exec_path//\`/\\\`}"
exec_path="${exec_path//%/%%}"
# Escape the command's backslashes again for the desktop-entry value layer.
exec_path="${exec_path//\\/\\\\}"
cat > "$prefix/share/applications/wezmux.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Wezmux
Comment=Terminal workspaces for coding agents
Exec="$exec_path"
Icon=wezmux
Terminal=false
Categories=System;TerminalEmulator;
EOF

echo "Wezmux installed to $app_dir"
echo "Launch: $prefix/bin/wezmux"
echo "Add $prefix/bin to PATH if it is not already present."
echo "Optional: make install-codex-hooks PREFIX=\"$prefix\""
