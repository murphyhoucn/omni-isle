# OmniIsle

OmniIsle is a Windows desktop tool that runs personal scripts from the right-click context menu and shows runtime status in a Dynamic Island style floating panel.

## Features

- Windows right-click integration (current user only, HKCU)
- Script execution with target file/folder path passing
- Floating island UI with queue/log/status display
- Safe install/uninstall registry utility

## Tech Stack

- Desktop container: Tauri v2 (Rust)
- UI: React + TypeScript + Framer Motion
- Script runtime: Python via Rust process execution

## Quick Start (Dev)

### 1) Frontend + Tauri dev mode

```powershell
cd omniisle-tauri
npm install
npm run tauri:dev
```

### 2) Build release app

```powershell
cd omniisle-tauri
npm install
npm run tauri:build
```

Release executable is generated at:

`omniisle-tauri/src-tauri/target/release/app.exe`

## Context Menu Install/Uninstall

Install right-click menu (safe: HKCU only):

```powershell
python tools/windows/context_menu_registry.py install --exe "omniisle-tauri\\src-tauri\\target\\release\\app.exe" --config "configs\\scripts_config.json"
```

Remove right-click menu:

```powershell
python tools/windows/context_menu_registry.py uninstall
```

Notes:

- Scope: `HKCU\\Software\\Classes\\...` (no machine-wide writes)
- Windows 11 may place entries under `Show more options`
- Script menu items come from `configs/scripts_config.json`

## Project Structure

```text
OmniIsle/
├─ configs/
│  ├─ omniisle_demo.py
│  ├─ scripts_config.json
│  └─ scripts/
│     ├─ mock_success.py
│     └─ mock_error.py
├─ omniisle-tauri/
│  ├─ src/
│  │  ├─ App.tsx
│  │  ├─ App.css
│  │  └─ index.css
│  └─ src-tauri/
│     ├─ src/
│     │  ├─ main.rs
│     │  └─ lib.rs
│     ├─ Cargo.toml
│     └─ tauri.conf.json
├─ tools/
│  └─ windows/
│     └─ context_menu_registry.py
├─ LICENSE
└─ README.md
```

## License

See `LICENSE`.



