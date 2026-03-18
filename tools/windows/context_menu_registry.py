r"""Safe context-menu installer for OmniIsle (current-user scope only by default).

This script writes registry keys under HKCU\Software\Classes so it does not modify
machine-wide system settings. It also provides an uninstall path that removes only the
keys created by this script.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

if sys.platform != "win32":
    raise SystemExit("This script only runs on Windows.")

import winreg


APP_MENU_NAME = "OmniIsle"
APP_ICON_FALLBACK = "imageres.dll,-5302"


@dataclass(frozen=True)
class RegistryRoot:
    name: str
    hive: int
    base_path: str
    target_placeholder: str


ROOTS = (
    RegistryRoot(
        name="file",
        hive=winreg.HKEY_CURRENT_USER,
        base_path=r"Software\Classes\*\shell\OmniIsle",
        target_placeholder="%1",
    ),
    RegistryRoot(
        name="directory",
        hive=winreg.HKEY_CURRENT_USER,
        base_path=r"Software\Classes\Directory\shell\OmniIsle",
        target_placeholder="%1",
    ),
)


def _normalize_script_id(script: str) -> str:
    stem = Path(script).stem.lower()
    safe = re.sub(r"[^a-z0-9_-]+", "-", stem).strip("-")
    return safe or "script"


def _set_str_value(key: winreg.HKEYType, name: str, value: str) -> None:
    winreg.SetValueEx(key, name, 0, winreg.REG_SZ, value)


def _delete_tree(hive: int, key_path: str) -> None:
    try:
        with winreg.OpenKey(hive, key_path, 0, winreg.KEY_READ | winreg.KEY_WRITE) as key:
            while True:
                try:
                    sub = winreg.EnumKey(key, 0)
                except OSError:
                    break
                _delete_tree(hive, f"{key_path}\\{sub}")
        winreg.DeleteKey(hive, key_path)
    except FileNotFoundError:
        return


def _load_scripts_config(config_path: Path) -> list[dict[str, str]]:
    raw = json.loads(config_path.read_text(encoding="utf-8"))
    scripts = raw.get("scripts", [])
    cleaned: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()

    for item in scripts:
        label = str(item.get("label", "")).strip()
        script = str(item.get("script", "")).strip()
        if not label or not script:
            continue
        key = (label, script)
        if key in seen:
            continue
        seen.add(key)
        cleaned.append({"label": label, "script": script})

    if not cleaned:
        raise ValueError(f"No valid scripts found in: {config_path}")

    return cleaned


def add_context_menu_entries(exe_path: Path, scripts: Iterable[dict[str, str]]) -> None:
    """Install OmniIsle cascading menu for current user."""
    exe = str(exe_path)
    icon = exe if exe_path.exists() else APP_ICON_FALLBACK

    for root in ROOTS:
        with winreg.CreateKeyEx(root.hive, root.base_path, 0, winreg.KEY_WRITE) as menu_key:
            _set_str_value(menu_key, "MUIVerb", APP_MENU_NAME)
            _set_str_value(menu_key, "Icon", icon)
            _set_str_value(menu_key, "SubCommands", "")

        shell_path = f"{root.base_path}\\shell"
        with winreg.CreateKeyEx(root.hive, shell_path, 0, winreg.KEY_WRITE):
            pass

        used_ids: set[str] = set()
        for item in scripts:
            base_id = _normalize_script_id(item["script"])
            script_id = base_id
            suffix = 2
            while script_id in used_ids:
                script_id = f"{base_id}-{suffix}"
                suffix += 1
            used_ids.add(script_id)

            action_path = f"{shell_path}\\{script_id}"
            with winreg.CreateKeyEx(root.hive, action_path, 0, winreg.KEY_WRITE) as action_key:
                _set_str_value(action_key, "MUIVerb", item["label"])

            command_path = f"{action_path}\\command"
            command = (
                f'"{exe}" --run-script "{item["script"]}" --target "{root.target_placeholder}"'
            )
            with winreg.CreateKeyEx(root.hive, command_path, 0, winreg.KEY_WRITE) as command_key:
                _set_str_value(command_key, "", command)


def uninstall_context_menu_entries() -> None:
    """Remove only OmniIsle keys that this installer created."""
    for root in ROOTS:
        _delete_tree(root.hive, root.base_path)


def _default_exe_path(repo_root: Path) -> Path:
    return repo_root / "omniisle-tauri" / "src-tauri" / "target" / "release" / "app.exe"


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    default_config = repo_root / "dev-configs" / "scripts_configs.json"
    default_exe = _default_exe_path(repo_root)

    parser = argparse.ArgumentParser(description="Install/uninstall OmniIsle right-click menu.")
    sub = parser.add_subparsers(dest="command", required=True)

    install = sub.add_parser("install", help="Install HKCU right-click menu entries")
    install.add_argument(
        "--exe",
        type=Path,
        default=default_exe,
        help=f"Path to OmniIsle executable (default: {default_exe})",
    )
    install.add_argument(
        "--config",
        type=Path,
        default=default_config,
        help=f"Path to scripts_config.json (default: {default_config})",
    )

    sub.add_parser("uninstall", help="Remove HKCU right-click menu entries")

    args = parser.parse_args()

    if args.command == "install":
        if not args.config.exists():
            raise SystemExit(f"Config file not found: {args.config}")

        scripts = _load_scripts_config(args.config)
        add_context_menu_entries(args.exe, scripts)

        print("[ok] Installed OmniIsle context menu under HKCU.")
        print("     Key scope: current user only (safe, no machine-wide writes).")
        print("     Note: Windows 11 may show it under 'Show more options'.")
        return 0

    if args.command == "uninstall":
        uninstall_context_menu_entries()
        print("[ok] Uninstalled OmniIsle context menu keys from HKCU.")
        return 0

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
