#!/usr/bin/env python3
"""Exercise Linux installation and hook setup without modifying user config."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs)


with tempfile.TemporaryDirectory(prefix="wezmux-install-", dir=os.environ.get("TMPDIR")) as scratch:
    scratch = Path(scratch)
    build = scratch / "build"
    build.mkdir()
    prefix = scratch / "prefix with spaces"
    # Use real system executables as installation fixtures, not fake GUI output.
    for binary in ("wezterm", "wezterm-gui", "wezterm-mux-server", "strip-ansi-escapes"):
        shutil.copy2(shutil.which("true"), build / binary)
    for _ in range(2):
        run("bash", str(ROOT / "bin/install-linux.sh"), str(prefix), str(build))
    app = prefix / "lib/wezmux"
    assert (prefix / "bin/wezmux").resolve() == app / "wezterm-gui"
    assert not (prefix / "bin/claude").exists()
    assert (app / "bin/hooks/omp/wezmux.js").is_file()
    assert (app / "bin/hooks/codex/on-stop.sh").is_file()
    run(str(prefix / "bin/wezmux"))
    desktop = prefix / "share/applications/wezmux.desktop"
    assert f'Exec="{app}/wezterm-gui"' in desktop.read_text()
    if shutil.which("desktop-file-validate"):
        run("desktop-file-validate", str(desktop))

    special_prefix = scratch / 'prefix with \\ $ " ` characters'
    run("bash", str(ROOT / "bin/install-linux.sh"), str(special_prefix), str(build))
    special_desktop = special_prefix / "share/applications/wezmux.desktop"
    if shutil.which("desktop-file-validate"):
        run("desktop-file-validate", str(special_desktop))
    # PyGObject is supplied by the system Python on Linux distributions.
    system_python = Path("/usr/bin/python3")
    if system_python.exists() and subprocess.run(
        [str(system_python), "-c", "from gi.repository import Gio"],
        capture_output=True,
    ).returncode == 0:
        run(str(system_python), "-c", """
import sys
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib
launcher = Gio.DesktopAppInfo.new_from_filename(sys.argv[1])
assert launcher is not None
# Older GLib get_executable() splits quoted paths at spaces; parse its decoded
# command line to verify the actual executable argument instead.
ok, argv = GLib.shell_parse_argv(launcher.get_commandline())
assert ok and argv == [sys.argv[2]], argv
""", str(special_desktop), str(special_prefix / "lib/wezmux/wezterm-gui"))

    real_bin = scratch / "real-bin"
    real_bin.mkdir()
    capture = scratch / "claude-settings.json"
    claude = real_bin / "claude"
    claude.write_text('#!/bin/sh\nprintf "%s" "$2" > "$CAPTURE_SETTINGS"\n')
    claude.chmod(0o755)
    env = dict(os.environ, WEZMUX="1", CAPTURE_SETTINGS=str(capture),
               PATH=f"{app}/bin:{real_bin}:" + os.environ["PATH"])
    run("bash", str(app / "bin/claude"), env=env)
    settings = json.loads(capture.read_text())
    sink = scratch / "claude-tty"
    hook_env = dict(env, WEZMUX_TTY=str(sink))
    command = settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
    run("sh", "-c", command, input="{}", start_new_session=True, env=hook_env)
    assert b"\x1b]9;Claude is working" in sink.read_bytes()

    for index, config in enumerate((
        None,
        '[features]\ncodex_hooks = false\n',
        '[features]\nhooks = false\n',
        '[features]\nother = true\n',
        'model = "example"\n',
    )):
        codex = scratch / f"codex-{index}"
        codex.mkdir()
        if config is not None:
            (codex / "config.toml").write_text(config)
        original = {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "echo unrelated"}]}]}}
        (codex / "hooks.json").write_text(json.dumps(original))
        env = dict(os.environ, CODEX_HOME=str(codex))
        installer = app / "bin/install-codex-hooks.sh"
        for _ in range(2):
            run("bash", str(installer), env=env)
        assert tomllib.loads((codex / "config.toml").read_text())["features"]["hooks"] is True
        hooks = json.loads((codex / "hooks.json").read_text())["hooks"]
        assert len(hooks["Stop"]) == 2
        assert hooks["Stop"][0] == original["hooks"]["Stop"][0]
        assert len(hooks["SessionStart"]) == 1
        assert not list(codex.glob(".wezmux-config.*"))
        sink = scratch / f"codex-tty-{index}"
        hook_env = dict(env, WEZMUX_TTY=str(sink))
        command = hooks["UserPromptSubmit"][0]["hooks"][0]["command"]
        run("sh", "-c", command, input="{}", start_new_session=True, env=hook_env)
        # A regular-file sink retains the last write; a real PTY streams both.
        assert b"\x1b]9;Codex is working" in sink.read_bytes()

    # Detached hooks must reach the explicitly supplied pane sink.
    sink = scratch / "tty-output"
    env = dict(os.environ, WEZMUX_TTY=str(sink))
    run("bash", str(app / "bin/hooks/on-prompt-submit.sh"),
        input="", start_new_session=True, env=env)
    assert b"\x1b]9;" in sink.read_bytes()

print("Linux install/reinstall, desktop launcher, Codex setup and detached hook checks passed")
