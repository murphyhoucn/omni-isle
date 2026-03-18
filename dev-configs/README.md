# dev-configs

This folder is the development template source for runtime data/config bootstrap.

## Structure

- `app_jsons.json`
  - Template for `C:\\Users\\<username>\\.omniisle\\app_configs.json`
- `scripts/`
  - Script source files (example: `hello.py`)
- `logs/`
  - Contains `.gitkeep`
- `tmp/`
  - Contains `.gitkeep`
  - Runtime temp build files are placed under user data `tmp` and auto-purged after 7 days
- `env_configs.json`
  - Template for runtime `env_config.json`
- `scripts_configs.json`
  - Template for runtime `scripts_config.json`

## Runtime mapping

- `.omniisle` keeps only `app_configs.json`
- User data root (default `%LOCALAPPDATA%\\OmniIsle`) keeps:
  - `scripts/`
  - `logs/`
  - `tmp/`
  - `env_config.json`
  - `scripts_config.json`
