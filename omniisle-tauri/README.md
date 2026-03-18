# omniisle-tauri

Frontend + Tauri shell for OmniIsle.

## Commands

```powershell
npm install
npm run tauri:dev
npm run tauri:build
```

Build outputs:

- Binary: `src-tauri/target/release/omniisle.exe`
- MSI: `src-tauri/target/release/bundle/msi/OmniIsle_*.msi`
- NSIS: `src-tauri/target/release/bundle/nsis/OmniIsle_*-setup.exe`

## Structure

```text
omniisle-tauri/
├─ src/
│  ├─ App.tsx
│  ├─ App.css
│  ├─ index.css
│  └─ main.tsx
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs
│  │  └─ lib.rs
│  ├─ capabilities/
│  ├─ icons/
│  ├─ Cargo.toml
│  ├─ Cargo.lock
│  └─ tauri.conf.json
├─ public/
├─ package.json
├─ tsconfig.app.json
├─ tsconfig.node.json
└─ vite.config.ts
```
