#!/bin/bash
# Install wezmux Codex hooks
# Merges into existing ~/.codex/hooks.json (preserves other hooks like cmux)
# and enables the hooks feature in ~/.codex/config.toml
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK_DIR="$SCRIPT_DIR/hooks/codex"
CODEX_DIR="${CODEX_HOME:-$HOME/.codex}"

mkdir -p "$CODEX_DIR"

# --- 1. Merge hooks into hooks.json ---
HOOKS_JSON="$CODEX_DIR/hooks.json"

is_wezmux_hook() {
    echo "$1" | jq -e '.. | .command? // empty | test("wezmux|on-prompt-submit\\.sh|on-stop\\.sh|on-pre-tool-use\\.sh|update-title\\.sh")' >/dev/null 2>&1
}

if ! command -v jq >/dev/null 2>&1; then
    echo "ERROR: jq is required to safely merge Codex hooks."
    echo "Install with: sudo apt-get install jq (Ubuntu), or brew install jq (macOS)"
    exit 1
fi

# Shell-quote the directory, then let jq escape JSON. Custom PREFIX paths
# may contain spaces, quotes or backslashes.
quoted_hook_dir="'${HOOK_DIR//\'/\'\\\'\'}'"
WEZMUX_SESSION_START=$(jq -n --arg dir "$quoted_hook_dir" '{hooks:[{type:"command",command:($dir+"/update-title.sh --hook --once"),timeout:5}]}')
WEZMUX_PROMPT_SUBMIT=$(jq -n --arg dir "$quoted_hook_dir" '{hooks:[{type:"command",command:($dir+"/on-prompt-submit.sh"),timeout:5},{type:"command",command:($dir+"/update-title.sh --hook"),timeout:70,async:true}]}')
WEZMUX_STOP=$(jq -n --arg dir "$quoted_hook_dir" '{hooks:[{type:"command",command:($dir+"/on-stop.sh"),timeout:5}]}')
WEZMUX_PRE_TOOL=$(jq -n --arg dir "$quoted_hook_dir" '{hooks:[{type:"command",command:($dir+"/on-pre-tool-use.sh"),timeout:5}]}')

if [ -f "$HOOKS_JSON" ] && [ -s "$HOOKS_JSON" ]; then
    # Existing hooks.json — merge our hooks in, replacing any previous wezmux entries

    # For each event type, filter out old wezmux entries then append ours
    MERGED=$(jq \
        --argjson session "$WEZMUX_SESSION_START" \
        --argjson prompt "$WEZMUX_PROMPT_SUBMIT" \
        --argjson stop "$WEZMUX_STOP" \
        --argjson tool "$WEZMUX_PRE_TOOL" \
        '
        # Helper: keep entries whose commands do not match wezmux paths
        def remove_wezmux:
            [ .[]? | select(
                (.hooks // []) | all(.command | test("wezmux|on-prompt-submit\\.sh|on-stop\\.sh|on-pre-tool-use\\.sh|update-title\\.sh") | not)
            ) ];

        .hooks.SessionStart = ((.hooks.SessionStart // []) | remove_wezmux) + [$session] |
        .hooks.UserPromptSubmit = ((.hooks.UserPromptSubmit // []) | remove_wezmux) + [$prompt] |
        .hooks.Stop = ((.hooks.Stop // []) | remove_wezmux) + [$stop] |
        .hooks.PreToolUse = ((.hooks.PreToolUse // []) | remove_wezmux) + [$tool]
        ' "$HOOKS_JSON")

    echo "$MERGED" | jq '.' > "$HOOKS_JSON"
    echo "Merged wezmux hooks into $HOOKS_JSON (existing hooks preserved)"
else
    # No existing hooks.json — create fresh
    jq -n \
        --argjson session "$WEZMUX_SESSION_START" \
        --argjson prompt "$WEZMUX_PROMPT_SUBMIT" \
        --argjson stop "$WEZMUX_STOP" \
        --argjson tool "$WEZMUX_PRE_TOOL" \
        '{
            hooks: {
                SessionStart: [$session],
                UserPromptSubmit: [$prompt],
                Stop: [$stop],
                PreToolUse: [$tool]
            }
        }' > "$HOOKS_JSON"
    echo "Created $HOOKS_JSON"
fi

# --- 2. Enable hooks feature in config.toml ---
# Codex renamed [features].codex_hooks → [features].hooks. Migrate old key if present.
CONFIG_TOML="$CODEX_DIR/config.toml"

# -i with a backup suffix is supported by both GNU and BSD sed.
# A unique scratch copy also avoids leaving backup files next to user config.
edit_config() {
    local scratch
    scratch=$(mktemp "$CODEX_DIR/.wezmux-config.XXXXXX")
    cp "$CONFIG_TOML" "$scratch"
    if sed -i.bak "$@" "$scratch"; then
        cat "$scratch" > "$CONFIG_TOML"
        rm -f "$scratch" "$scratch.bak"
    else
        rm -f "$scratch" "$scratch.bak"
        return 1
    fi
}

if [ -f "$CONFIG_TOML" ]; then
    if grep -q 'codex_hooks' "$CONFIG_TOML"; then
        edit_config -E 's/codex_hooks[[:space:]]*=[[:space:]]*(true|false)/hooks = true/'
        echo "Migrated codex_hooks → hooks in $CONFIG_TOML"
    elif grep -qE '^[[:space:]]*hooks[[:space:]]*=' "$CONFIG_TOML"; then
        edit_config -E 's/^([[:space:]]*)hooks[[:space:]]*=[[:space:]]*false/\1hooks = true/'
        echo "Enabled hooks in $CONFIG_TOML"
    elif grep -q '\[features\]' "$CONFIG_TOML"; then
        edit_config '/\[features\]/a\
hooks = true
'
        echo "Added hooks = true to [features] in $CONFIG_TOML"
    else
        echo "" >> "$CONFIG_TOML"
        echo "[features]" >> "$CONFIG_TOML"
        echo "hooks = true" >> "$CONFIG_TOML"
        echo "Added [features] section with hooks = true to $CONFIG_TOML"
    fi
else
    cat > "$CONFIG_TOML" <<'EOF'
[features]
hooks = true
EOF
    echo "Created $CONFIG_TOML with hooks = true"
fi

# --- 3. Make hook scripts executable ---
chmod +x "$HOOK_DIR"/*.sh
echo ""
echo "Wezmux Codex hooks installed successfully."
echo "Restart Codex for hooks to take effect."
