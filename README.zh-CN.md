# OmniIsle

![OmniIsle 海报](assets/OmniIsle.png)

[English](README.md)

OmniIsle 是一个 Windows 桌面工具，可通过右键菜单运行个人脚本，并在 Dynamic Island 风格的悬浮面板中显示运行状态。

## 功能特性

- Windows 右键集成（仅当前用户，HKCU）
- 脚本执行时可传入目标文件或文件夹路径
- 悬浮岛界面，显示队列、日志与状态
- 安全的右键菜单安装与卸载工具

## EXE 用户快速使用（无需开发环境）

本节面向仅使用安装包或可执行文件的用户。

### 1. 安装与启动

- 运行安装包：`OmniIsle_0.1.0_x64-setup.exe` 或 `OmniIsle_0.1.0_x64_en-US.msi`
- 从开始菜单启动 `OmniIsle`（或直接运行 `omniisle.exe`）

### 2. 基本使用流程

- 打开 OmniIsle 设置面板
- 在脚本管理中添加脚本条目（示例：`hello.py`）
- 启用右键菜单集成
- 在资源管理器中右键文件或文件夹并选择 OmniIsle 脚本
- 在悬浮面板中查看运行状态与日志

### 3. 用户数据存储位置

- 应用设置文件：`C:\Users\<username>\.omniisle\app_configs.json`
- 运行时用户数据根目录（默认）：`%LOCALAPPDATA%\OmniIsle`
- 其中包含：
  - `scripts/`
  - `logs/`
  - `tmp/`
  - `env_config.json`
  - `scripts_config.json`

### 4. 常见说明

- Windows 11 中右键项可能显示在 `显示更多选项` 下
- 右键菜单范围仅为当前用户（`HKCU`）
- 如果右键入口缺失，打开 OmniIsle 后重新关闭再开启一次右键集成

## 技术栈

- 桌面容器：Tauri v2（Rust）
- 前端：React + TypeScript + Framer Motion
- 脚本运行：Rust 进程调用 Python/Node/Java/C/C++

## 开发快速开始

### 1. 前端 + Tauri 开发模式

```powershell
cd omniisle-tauri
npm install
npm run tauri:dev
```

### 2. 构建发布版本

```powershell
cd omniisle-tauri
npm install
npm run tauri:build
```

发布可执行文件默认在：

`omniisle-tauri/src-tauri/target/release/omniisle.exe`

## 右键菜单安装/卸载

安装右键菜单（安全：仅 HKCU）：

```powershell
python tools/windows/context_menu_registry.py install --exe "omniisle-tauri\\src-tauri\\target\\release\\omniisle.exe" --config "dev-configs\\scripts_configs.json"
```

卸载右键菜单：

```powershell
python tools/windows/context_menu_registry.py uninstall
```

说明：

- 写入范围：`HKCU\\Software\\Classes\\...`（不会写入全局）
- Windows 11 可能在 `显示更多选项` 下显示
- 菜单脚本来源：`dev-configs/scripts_configs.json`

## 项目结构

```text
OmniIsle/
├─ dev-configs/
│  ├─ app_jsons.json
│  ├─ env_configs.json
│  ├─ logs/
│  ├─ scripts_configs.json
│  ├─ tmp/
│  └─ scripts/
│     └─ hello.py
├─ omniisle-tauri/
│  ├─ src/
│  │  ├─ App.tsx
│  │  ├─ App.css
│  │  └─ index.css
│  ├─ dist/
│  ├─ public/
│  └─ src-tauri/
│     ├─ capabilities/
│     ├─ src/
│     │  ├─ main.rs
│     │  └─ lib.rs
│     ├─ icons/
│     ├─ Cargo.toml
│     ├─ Cargo.lock
│     └─ tauri.conf.json
├─ release/
│  └─ setup.exe
├─ tools/
│  ├─ dev/
│  │  └─ setup-dev-template.ps1
│  └─ windows/
│     └─ context_menu_registry.py
├─ LICENSE
├─ README.md
└─ README.zh-CN.md
```

## 许可证

见 `LICENSE`。
