use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Emitter;
use tauri::Manager;
#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::POINT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
  GetMonitorInfoW, MonitorFromPoint, MONITOR_DEFAULTTONEAREST, MONITORINFO,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

#[derive(Serialize)]
struct ScriptRunResult {
  success: bool,
  exit_code: i32,
  stdout: String,
  stderr: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ScriptMenuItem {
  label: String,
  script: String,
}

#[derive(Serialize, Deserialize)]
struct ScriptConfigFile {
  #[serde(default)]
  scripts: Vec<ScriptMenuItem>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SystemIntegrationConfig {
  auto_start_enabled: bool,
  context_menu_enabled: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct EnvironmentConfig {
  name: String,
  executable_path: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppConfigFile {
  #[serde(default = "default_system_integration")]
  system_integration: SystemIntegrationConfig,
  #[serde(default = "default_environments")]
  environments: Vec<EnvironmentConfig>,
  #[serde(default = "default_idle_hide_seconds")]
  idle_hide_seconds: u32,
}

#[derive(Serialize, Clone)]
struct StartupJob {
  script: String,
  target_path: Option<String>,
}

struct AppState {
  startup_job: Mutex<Option<StartupJob>>,
}

#[derive(Serialize, Clone)]
struct ScriptLogEvent {
  stream: String,
  line: String,
}

fn default_system_integration() -> SystemIntegrationConfig {
  SystemIntegrationConfig {
    auto_start_enabled: false,
    context_menu_enabled: false,
  }
}

fn default_environments() -> Vec<EnvironmentConfig> {
  vec![
    EnvironmentConfig {
      name: "PYTHON".to_string(),
      executable_path: "python".to_string(),
    },
    EnvironmentConfig {
      name: "NODE".to_string(),
      executable_path: "node".to_string(),
    },
    EnvironmentConfig {
      name: "GCC/G++".to_string(),
      executable_path: "gcc".to_string(),
    },
    EnvironmentConfig {
      name: "JAVA".to_string(),
      executable_path: "java".to_string(),
    },
  ]
}

fn default_app_config() -> AppConfigFile {
  AppConfigFile {
    system_integration: default_system_integration(),
    environments: default_environments(),
    idle_hide_seconds: default_idle_hide_seconds(),
  }
}

fn default_idle_hide_seconds() -> u32 {
  60
}

fn normalize_idle_hide_seconds(seconds: u32) -> Result<u32, String> {
  match seconds {
    0 | 5 | 10 | 30 | 60 | 120 => Ok(seconds),
    _ => Err("自动隐藏时长仅支持 0/5/10/30/60/120 秒".to_string()),
  }
}

fn resolve_script_path(script_name: &str) -> Result<PathBuf, String> {
  let candidate = resolve_scripts_dir()?.join(script_name);
  if candidate.exists() {
    return Ok(candidate);
  }

  Err(format!("未找到脚本文件: {script_name}"))
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
  if seen.insert(path.clone()) {
    paths.push(path);
  }
}

fn candidate_roots_from(start: &Path) -> Vec<PathBuf> {
  let mut roots = Vec::<PathBuf>::new();
  let mut seen = HashSet::<PathBuf>::new();
  push_unique_path(&mut roots, &mut seen, start.to_path_buf());
  if let Some(parent) = start.parent() {
    push_unique_path(&mut roots, &mut seen, parent.to_path_buf());
    if let Some(grand_parent) = parent.parent() {
      push_unique_path(&mut roots, &mut seen, grand_parent.to_path_buf());
    }
  }
  roots
}

fn workspace_candidate_roots() -> Result<Vec<PathBuf>, String> {
  let cwd = std::env::current_dir().map_err(|e| format!("无法读取当前目录: {e}"))?;
  Ok(candidate_roots_from(&cwd))
}

fn executable_candidate_roots() -> Result<Vec<PathBuf>, String> {
  let exe = std::env::current_exe().map_err(|e| format!("无法读取当前程序路径: {e}"))?;
  let exe_dir = exe
    .parent()
    .ok_or_else(|| "无法解析当前程序目录".to_string())?;
  Ok(candidate_roots_from(exe_dir))
}

fn first_existing_configs_dir(roots: &[PathBuf]) -> Option<PathBuf> {
  for root in roots {
    let candidate = root.join("configs");
    if candidate.exists() && candidate.is_dir() {
      return Some(candidate);
    }
  }

  None
}

fn first_existing_resource_configs_dir(roots: &[PathBuf]) -> Option<PathBuf> {
  for root in roots {
    let candidate = root.join("resources").join("configs");
    if candidate.exists() && candidate.is_dir() {
      return Some(candidate);
    }
  }

  None
}

fn local_data_root() -> Result<PathBuf, String> {
  #[cfg(target_os = "windows")]
  {
    let base = std::env::var_os("LOCALAPPDATA")
      .ok_or_else(|| "未找到 LOCALAPPDATA 环境变量".to_string())?;
    return Ok(PathBuf::from(base).join("OmniIsle"));
  }

  #[cfg(not(target_os = "windows"))]
  {
    let home = std::env::var_os("HOME")
      .ok_or_else(|| "未找到 HOME 环境变量".to_string())?;
    Ok(PathBuf::from(home).join(".omniisle"))
  }
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
  }
  Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
  std::fs::create_dir_all(target).map_err(|e| format!("创建目录失败: {e}"))?;
  let entries = std::fs::read_dir(source).map_err(|e| format!("读取目录失败: {e}"))?;

  for entry in entries {
    let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
    let source_path = entry.path();
    let target_path = target.join(entry.file_name());
    if source_path.is_dir() {
      copy_dir_recursive(&source_path, &target_path)?;
    } else {
      if target_path.exists() {
        continue;
      }
      ensure_parent_dir(&target_path)?;
      std::fs::copy(&source_path, &target_path)
        .map_err(|e| format!("复制文件失败: {e}"))?;
    }
  }

  Ok(())
}

fn ensure_runtime_configs_seed(source_configs: Option<&Path>) -> Result<PathBuf, String> {
  let runtime_root = local_data_root()?;
  let runtime_configs = runtime_root.join("configs");
  let runtime_scripts = runtime_configs.join("scripts");
  std::fs::create_dir_all(&runtime_scripts).map_err(|e| format!("创建运行时配置目录失败: {e}"))?;

  if let Some(source_configs) = source_configs {
    let source_scripts = source_configs.join("scripts");
    if !runtime_configs.join("app_configs.json").exists() && source_configs.join("app_configs.json").exists() {
      std::fs::copy(source_configs.join("app_configs.json"), runtime_configs.join("app_configs.json"))
        .map_err(|e| format!("初始化 app_configs.json 失败: {e}"))?;
    }
    if !runtime_configs.join("scripts_config.json").exists() && source_configs.join("scripts_config.json").exists() {
      std::fs::copy(source_configs.join("scripts_config.json"), runtime_configs.join("scripts_config.json"))
        .map_err(|e| format!("初始化 scripts_config.json 失败: {e}"))?;
    }
    if source_scripts.exists() && source_scripts.is_dir() {
      copy_dir_recursive(&source_scripts, &runtime_scripts)?;
    }
  }

  if !runtime_configs.join("scripts_config.json").exists() {
    let body = serde_json::to_string_pretty(&ScriptConfigFile { scripts: Vec::new() })
      .map_err(|e| format!("初始化脚本配置失败: {e}"))?;
    std::fs::write(runtime_configs.join("scripts_config.json"), body)
      .map_err(|e| format!("写入脚本配置失败: {e}"))?;
  }

  Ok(runtime_configs)
}

fn resolve_scripts_dir() -> Result<PathBuf, String> {
  Ok(resolve_configs_dir()?.join("scripts"))
}

fn resolve_script_config_path() -> Result<PathBuf, String> {
  Ok(resolve_configs_dir()?.join("scripts_config.json"))
}

fn resolve_configs_dir() -> Result<PathBuf, String> {
  let workspace_roots = workspace_candidate_roots()?;
  if let Some(configs) = first_existing_configs_dir(&workspace_roots) {
    return Ok(configs);
  }

  let exe_roots = executable_candidate_roots()?;
  let source_configs = first_existing_resource_configs_dir(&exe_roots)
    .or_else(|| first_existing_configs_dir(&exe_roots));

  ensure_runtime_configs_seed(source_configs.as_deref())
}

fn resolve_app_config_path() -> Result<PathBuf, String> {
  Ok(resolve_configs_dir()?.join("app_configs.json"))
}

// ─── Daily-rolling log ───────────────────────────────────────────────────

fn ensure_workspace_logs_dir() -> Result<Option<PathBuf>, String> {
  let workspace_roots = workspace_candidate_roots()?;
  if let Some(configs_dir) = first_existing_configs_dir(&workspace_roots) {
    let root = configs_dir
      .parent()
      .ok_or_else(|| "无法解析工作区根目录".to_string())?;
    let logs_dir = root.join("logs");
    std::fs::create_dir_all(&logs_dir).map_err(|e| format!("创建 logs 目录失败: {e}"))?;
    return Ok(Some(logs_dir));
  }

  Ok(None)
}

fn ensure_runtime_logs_dir() -> Result<PathBuf, String> {
  let logs_dir = local_data_root()?.join("logs");
  std::fs::create_dir_all(&logs_dir).map_err(|e| format!("创建运行时 logs 目录失败: {e}"))?;
  Ok(logs_dir)
}

fn resolve_logs_dir() -> Option<PathBuf> {
  if let Ok(Some(logs_dir)) = ensure_workspace_logs_dir() {
    return Some(logs_dir);
  }

  ensure_runtime_logs_dir().ok()
}

// Returns (YYYY-MM-DD, HH:MM:SS) using the local wall-clock time.
fn local_time() -> (String, String) {
  #[cfg(target_os = "windows")]
  {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    let date = format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay);
    let time = format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond);
    return (date, time);
  }
  #[cfg(not(target_os = "windows"))]
  {
    // Fallback: UTC from SystemTime (non-Windows platforms)
    let secs = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_secs())
      .unwrap_or(0);
    let h = (secs % 86400) / 3600;
    let mi = (secs % 3600) / 60;
    let s = secs % 60;
    ("unknown".to_string(), format!("{:02}:{:02}:{:02}", h, mi, s))
  }
}

fn today_log_path() -> Option<PathBuf> {
  let logs_dir = resolve_logs_dir()?;
  let (date, _) = local_time();
  Some(logs_dir.join(format!("{}.log", date)))
}

fn write_log(level: &str, message: &str) {
  let Some(path) = today_log_path() else {
    return;
  };
  let (_, time) = local_time();
  let line = format!("[{}] [{}] {}\n", time, level, message);
  if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
    let _ = f.write_all(line.as_bytes());
  }
  purge_old_logs();
}

fn purge_old_logs() {
  let Some(logs_dir) = resolve_logs_dir() else {
    return;
  };
  let Ok(entries) = std::fs::read_dir(&logs_dir) else {
    return;
  };
  let mut log_files: Vec<PathBuf> = entries
    .filter_map(|e| e.ok())
    .map(|e| e.path())
    .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("log"))
    .collect();
  log_files.sort();
  const KEEP_DAYS: usize = 30;
  if log_files.len() > KEEP_DAYS {
    for old in log_files.iter().take(log_files.len() - KEEP_DAYS) {
      let _ = std::fs::remove_file(old);
    }
  }
}
// ─── End log ─────────────────────────────────────────────────────────────

fn load_script_catalog_from_file() -> Result<Vec<ScriptMenuItem>, String> {
  let config_path = resolve_script_config_path()?;
  let raw = std::fs::read_to_string(&config_path)
    .map_err(|e| format!("读取脚本配置失败: {e}"))?;
  let parsed: ScriptConfigFile =
    serde_json::from_str(&raw).map_err(|e| format!("解析脚本配置失败: {e}"))?;

  let mut items = Vec::<ScriptMenuItem>::new();
  for item in parsed.scripts {
    let label = item.label.trim().to_string();
    let script = item.script.trim().to_string();
    if !label.is_empty() && !script.is_empty() {
      items.push(ScriptMenuItem { label, script });
    }
  }

  Ok(items)
}

fn save_script_catalog(items: &[ScriptMenuItem]) -> Result<(), String> {
  let config_path = resolve_script_config_path()?;
  let body = serde_json::to_string_pretty(&ScriptConfigFile {
    scripts: items.to_vec(),
  })
  .map_err(|e| format!("序列化脚本配置失败: {e}"))?;
  std::fs::write(&config_path, body).map_err(|e| format!("写入脚本配置失败: {e}"))
}

fn ensure_safe_script_name(script_name: &str) -> Result<(), String> {
  let name = script_name.trim();
  if name.is_empty() {
    return Err("脚本名不能为空".to_string());
  }
  if name.contains('/') || name.contains('\\') || name.contains("..") {
    return Err("脚本名不合法，不能包含路径信息".to_string());
  }
  if Path::new(name)
    .file_name()
    .and_then(|s| s.to_str())
    .unwrap_or("")
    != name
  {
    return Err("脚本名不合法".to_string());
  }
  Ok(())
}

fn script_template(script_name: &str) -> String {
  let ext = Path::new(script_name)
    .extension()
    .and_then(|s| s.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();

  let stem = Path::new(script_name)
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("script");

  match ext.as_str() {
    "py" => "print(\"hello from OmniIsle python script\")\n".to_string(),
    "js" | "mjs" | "cjs" => "console.log('hello from OmniIsle node script')\n".to_string(),
    "java" => {
      let class_name = java_sanitize_class_name(stem);
      format!(
        "public class {} {{\n  public static void main(String[] args) {{\n    System.out.println(\"hello from OmniIsle java script\");\n  }}\n}}\n",
        class_name
      )
    }
    "c" => {
      "#include <stdio.h>\n\nint main(int argc, char** argv) {\n  printf(\"hello from OmniIsle c script\\n\");\n  return 0;\n}\n".to_string()
    }
    "cpp" | "cc" | "cxx" => {
      "#include <iostream>\n\nint main(int argc, char** argv) {\n  std::cout << \"hello from OmniIsle cpp script\" << std::endl;\n  return 0;\n}\n".to_string()
    }
    _ => String::new(),
  }
}

fn configured_env_value(env_name: &str) -> Option<String> {
  if let Ok(config) = load_or_init_app_config() {
    if let Some(found) = config
      .environments
      .iter()
      .find(|item| item.name.trim().eq_ignore_ascii_case(env_name))
    {
      let configured = found.executable_path.trim();
      if !configured.is_empty() {
        return Some(configured.to_string());
      }
    }
  }

  None
}

fn configured_env_value_any(env_names: &[&str]) -> Option<String> {
  for env_name in env_names {
    if let Some(configured) = configured_env_value(env_name) {
      return Some(configured);
    }
  }

  None
}

fn normalize_executable_name(executable_name: &str) -> String {
  if cfg!(target_os = "windows") {
    let lowered = executable_name.to_ascii_lowercase();
    if lowered.ends_with(".exe") {
      return executable_name.to_string();
    }
    return format!("{}.exe", executable_name);
  }

  executable_name.to_string()
}

fn resolve_env_command(configured: &str, executable_name: &str) -> String {
  let trimmed = configured.trim();
  if trimmed.is_empty() {
    return executable_name.to_string();
  }

  let configured_path = Path::new(trimmed);
  if configured_path.is_dir() {
    let candidate_name = normalize_executable_name(executable_name);
    let candidate = configured_path.join(&candidate_name);
    if candidate.exists() {
      return candidate.to_string_lossy().to_string();
    }
    return configured_path.join(executable_name).to_string_lossy().to_string();
  }

  trimmed.to_string()
}

fn resolve_related_env_command(configured: &str, executable_name: &str) -> String {
  let trimmed = configured.trim();
  if trimmed.is_empty() {
    return executable_name.to_string();
  }

  let configured_path = Path::new(trimmed);
  if configured_path.is_dir() {
    return resolve_env_command(trimmed, executable_name);
  }

  if let Some(parent) = configured_path.parent() {
    let candidate = parent.join(normalize_executable_name(executable_name));
    if candidate.exists() {
      return candidate.to_string_lossy().to_string();
    }
  }

  executable_name.to_string()
}

fn configured_env_bin_dir_any(env_names: &[&str]) -> Option<PathBuf> {
  let configured = configured_env_value_any(env_names)?;
  let configured_path = Path::new(configured.trim());
  if configured_path.is_dir() {
    return Some(configured_path.to_path_buf());
  }
  configured_path.parent().map(|path| path.to_path_buf())
}

fn infer_toolchain_root(bin_dir: &Path) -> Option<PathBuf> {
  if bin_dir
    .file_name()
    .and_then(|name| name.to_str())
    .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
  {
    return bin_dir.parent().map(|path| path.to_path_buf());
  }

  let nested_bin = bin_dir.join("bin");
  if nested_bin.exists() && nested_bin.is_dir() {
    return Some(bin_dir.to_path_buf());
  }

  None
}

fn prepend_command_path(command: &mut Command, dir: &Path) {
  let key = if cfg!(target_os = "windows") { "Path" } else { "PATH" };
  let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
  let existing = std::env::var_os(key).unwrap_or_default();
  let mut merged = dir.as_os_str().to_os_string();
  if !existing.is_empty() {
    merged.push(sep);
    merged.push(existing);
  }
  command.env(key, merged);
}

fn apply_mingw_toolchain_env(command: &mut Command) {
  if let Some(bin_dir) = configured_env_bin_dir_any(&["GCC/G++", "GCC"]) {
    prepend_command_path(command, &bin_dir);
    command.arg("-B").arg(&bin_dir);
    if let Some(root_dir) = infer_toolchain_root(&bin_dir) {
      command.arg(format!("--sysroot={}", root_dir.to_string_lossy()));
    }
  }
}

fn apply_mingw_runtime_env(command: &mut Command) {
  if let Some(bin_dir) = configured_env_bin_dir_any(&["GCC/G++", "GCC"]) {
    prepend_command_path(command, &bin_dir);
  }
}

fn choose_env_executable(env_name: &str, fallback: &str) -> String {
  if let Some(configured) = configured_env_value(env_name) {
    return resolve_env_command(&configured, fallback);
  }

  fallback.to_string()
}

fn choose_env_executable_any(env_names: &[&str], fallback: &str) -> String {
  if let Some(configured) = configured_env_value_any(env_names) {
    return resolve_env_command(&configured, fallback);
  }

  fallback.to_string()
}

fn choose_cpp_executable() -> String {
  if let Some(configured) = configured_env_value_any(&["GCC/G++", "GCC"]) {
    return resolve_env_command(&configured, "g++");
  }

  "g++".to_string()
}

fn choose_c_executable() -> String {
  choose_env_executable_any(&["GCC/G++", "GCC"], "gcc")
}

fn choose_java_runtime_executable() -> String {
  choose_env_executable("JAVA", "java")
}

fn choose_java_compiler_executable() -> String {
  if let Some(configured) = configured_env_value("JAVA") {
    return resolve_related_env_command(&configured, "javac");
  }

  "javac".to_string()
}

fn java_sanitize_class_name(raw: &str) -> String {
  let mut class_name = String::new();
  for (index, ch) in raw.chars().enumerate() {
    let valid = if index == 0 {
      ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
    } else {
      ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    };
    class_name.push(if valid { ch } else { '_' });
  }

  if class_name.is_empty() {
    "Main".to_string()
  } else if class_name.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
    format!("_{}", class_name)
  } else {
    class_name
  }
}

fn java_package_name(source: &str) -> Option<String> {
  for line in source.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with("package ") && trimmed.ends_with(';') {
      let package_name = trimmed
        .trim_start_matches("package ")
        .trim_end_matches(';')
        .trim();
      if !package_name.is_empty() {
        return Some(package_name.to_string());
      }
    }
  }

  None
}

fn java_declared_class_name(source: &str) -> Option<String> {
  for line in source.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with('*') {
      continue;
    }

    for marker in ["public class ", "class ", "public final class ", "final class "] {
      if let Some(rest) = trimmed.split_once(marker).map(|(_, rest)| rest) {
        let candidate = rest
          .chars()
          .take_while(|ch| ch == &'_' || ch == &'$' || ch.is_ascii_alphanumeric())
          .collect::<String>();
        if !candidate.is_empty() {
          return Some(candidate);
        }
      }
    }
  }

  None
}

fn java_compile_source_details(script_path: &Path) -> Result<(String, String, String), String> {
  let source = std::fs::read_to_string(script_path)
    .map_err(|e| format!("读取 Java 源码失败: {e}"))?;
  let fallback_name = script_path
    .file_stem()
    .and_then(|s| s.to_str())
    .map(java_sanitize_class_name)
    .unwrap_or_else(|| "Main".to_string());
  let simple_class_name = java_declared_class_name(&source).unwrap_or(fallback_name);
  let qualified_name = if let Some(package_name) = java_package_name(&source) {
    format!("{}.{}", package_name, simple_class_name)
  } else {
    simple_class_name.clone()
  };

  Ok((source, simple_class_name, qualified_name))
}

fn run_command_with_stream(
  window: &tauri::Window,
  mut command: Command,
  launch_error_prefix: &str,
) -> Result<ScriptRunResult, String> {
  let mut child = command
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|e| format!("{launch_error_prefix}: {e}"))?;

  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| "无法获取 stdout 管道".to_string())?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| "无法获取 stderr 管道".to_string())?;

  let (tx, rx) = mpsc::channel::<(String, String)>();

  let tx_out = tx.clone();
  let out_handle = thread::spawn(move || {
    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
      let _ = tx_out.send(("stdout".to_string(), line));
    }
  });

  let tx_err = tx.clone();
  let err_handle = thread::spawn(move || {
    let reader = BufReader::new(stderr);
    for line in reader.lines().map_while(Result::ok) {
      let _ = tx_err.send(("stderr".to_string(), line));
    }
  });

  drop(tx);

  let mut stdout_all = String::new();
  let mut stderr_all = String::new();

  for (stream, line) in rx {
    let payload = ScriptLogEvent {
      stream: stream.clone(),
      line: line.clone(),
    };
    let _ = window.emit("script-log", payload);

    if stream == "stdout" {
      stdout_all.push_str(&line);
      stdout_all.push('\n');
    } else {
      stderr_all.push_str(&line);
      stderr_all.push('\n');
    }
  }

  let _ = out_handle.join();
  let _ = err_handle.join();

  let status = child
    .wait()
    .map_err(|e| format!("等待进程结束失败: {e}"))?;

  Ok(ScriptRunResult {
    success: status.success(),
    exit_code: status.code().unwrap_or(-1),
    stdout: stdout_all,
    stderr: stderr_all,
  })
}

fn open_path_in_default_editor(path: &Path) -> Result<(), String> {
  #[cfg(target_os = "windows")]
  {
    Command::new("cmd")
      .args(["/c", "start", "", &path.to_string_lossy()])
      .spawn()
      .map_err(|e| format!("打开默认编辑器失败: {e}"))?;
    return Ok(());
  }

  #[cfg(not(target_os = "windows"))]
  {
    let _ = path;
    Err("当前平台暂不支持打开默认编辑器".to_string())
  }
}

fn load_or_init_app_config() -> Result<AppConfigFile, String> {
  let config_path = resolve_app_config_path()?;

  if !config_path.exists() {
    let initial = default_app_config();
    save_app_config(&initial)?;
    return Ok(initial);
  }

  let raw = std::fs::read_to_string(&config_path)
    .map_err(|e| format!("读取应用配置失败: {e}"))?;
  serde_json::from_str::<AppConfigFile>(&raw).map_err(|e| format!("解析应用配置失败: {e}"))
}

fn save_app_config(config: &AppConfigFile) -> Result<(), String> {
  let config_path = resolve_app_config_path()?;
  let body =
    serde_json::to_string_pretty(config).map_err(|e| format!("序列化应用配置失败: {e}"))?;
  std::fs::write(&config_path, body).map_err(|e| format!("写入应用配置失败: {e}"))
}

#[cfg(target_os = "windows")]
fn normalize_script_id(script: &str) -> String {
  let stem = Path::new(script)
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("script")
    .to_ascii_lowercase();

  let mut out = String::new();
  let mut last_dash = false;
  for ch in stem.chars() {
    let is_safe = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
    if is_safe {
      out.push(ch);
      last_dash = false;
    } else if !last_dash {
      out.push('-');
      last_dash = true;
    }
  }

  out.trim_matches('-').to_string()
}

#[cfg(target_os = "windows")]
fn apply_auto_start(enabled: bool) -> Result<(), String> {
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let run_path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
  let (run_key, _) = hkcu
    .create_subkey(run_path)
    .map_err(|e| format!("打开启动项注册表失败: {e}"))?;

  if enabled {
    let exe = std::env::current_exe().map_err(|e| format!("读取当前程序路径失败: {e}"))?;
    let value = format!("\"{}\"", exe.to_string_lossy());
    run_key
      .set_value("OmniIsle", &value)
      .map_err(|e| format!("写入开机启动失败: {e}"))?;
  } else {
    let _ = run_key.delete_value("OmniIsle");
  }

  Ok(())
}

#[cfg(target_os = "windows")]
fn create_context_menu_root(
  hkcu: &RegKey,
  base_path: &str,
  target_placeholder: &str,
  exe: &str,
  scripts: &[ScriptMenuItem],
) -> Result<(), String> {
  let (menu_key, _) = hkcu
    .create_subkey(base_path)
    .map_err(|e| format!("创建右键菜单根节点失败: {e}"))?;
  menu_key
    .set_value("MUIVerb", &"OmniIsle")
    .map_err(|e| format!("写入 MUIVerb 失败: {e}"))?;
  menu_key
    .set_value("Icon", &exe)
    .map_err(|e| format!("写入图标失败: {e}"))?;
  menu_key
    .set_value("SubCommands", &"")
    .map_err(|e| format!("写入 SubCommands 失败: {e}"))?;

  let shell_path = format!("{}\\shell", base_path);
  let _ = hkcu
    .create_subkey(&shell_path)
    .map_err(|e| format!("创建 shell 子节点失败: {e}"))?;

  let mut used_ids: Vec<String> = Vec::new();

  for item in scripts {
    let mut script_id = normalize_script_id(&item.script);
    if script_id.is_empty() {
      script_id = "script".to_string();
    }

    let base = script_id.clone();
    let mut suffix = 2usize;
    while used_ids.iter().any(|x| x == &script_id) {
      script_id = format!("{}-{}", base, suffix);
      suffix += 1;
    }
    used_ids.push(script_id.clone());

    let action_path = format!("{}\\{}", shell_path, script_id);
    let (action_key, _) = hkcu
      .create_subkey(&action_path)
      .map_err(|e| format!("创建菜单项失败: {e}"))?;
    action_key
      .set_value("MUIVerb", &item.label)
      .map_err(|e| format!("写入菜单项名称失败: {e}"))?;

    let command_path = format!("{}\\command", action_path);
    let command = format!(
      "\"{}\" --run-script \"{}\" --target \"{}\"",
      exe, item.script, target_placeholder
    );
    let (command_key, _) = hkcu
      .create_subkey(&command_path)
      .map_err(|e| format!("创建命令项失败: {e}"))?;
    command_key
      .set_value("", &command)
      .map_err(|e| format!("写入命令失败: {e}"))?;
  }

  Ok(())
}

#[cfg(target_os = "windows")]
fn apply_context_menu(enabled: bool) -> Result<(), String> {
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let file_root = "Software\\Classes\\*\\shell\\OmniIsle";
  let dir_root = "Software\\Classes\\Directory\\shell\\OmniIsle";

  let _ = hkcu.delete_subkey_all(file_root);
  let _ = hkcu.delete_subkey_all(dir_root);

  if !enabled {
    return Ok(());
  }

  let scripts = load_script_catalog_from_file()?;
  let exe = std::env::current_exe()
    .map_err(|e| format!("读取当前程序路径失败: {e}"))?
    .to_string_lossy()
    .to_string();

  create_context_menu_root(&hkcu, file_root, "%1", &exe, &scripts)?;
  create_context_menu_root(&hkcu, dir_root, "%1", &exe, &scripts)?;

  Ok(())
}

fn choose_python_executable() -> String {
  let default = if cfg!(target_os = "windows") {
    "python"
  } else {
    "python3"
  };
  choose_env_executable("PYTHON", default)
}

#[cfg(target_os = "windows")]
fn place_window_on_cursor_monitor_top_center(window: &tauri::WebviewWindow) {
  let mut cursor = POINT { x: 0, y: 0 };
  // If cursor cannot be read, keep default Tauri placement.
  if unsafe { GetCursorPos(&mut cursor) } == 0 {
    return;
  }

  let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
  if monitor.is_null() {
    return;
  }

  let mut info = MONITORINFO {
    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
    rcMonitor: RECT {
      left: 0,
      top: 0,
      right: 0,
      bottom: 0,
    },
    rcWork: RECT {
      left: 0,
      top: 0,
      right: 0,
      bottom: 0,
    },
    dwFlags: 0,
  };

  if unsafe { GetMonitorInfoW(monitor, &mut info as *mut MONITORINFO) } == 0 {
    return;
  }

  let width = window
    .outer_size()
    .ok()
    .map(|size| size.width as i32)
    .filter(|w| *w > 0)
    .unwrap_or(700);
  let top_gap = 8i32;
  let work = info.rcWork;
  let x = work.left + ((work.right - work.left - width) / 2);
  let y = work.top + top_gap;

  let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
}

#[cfg(not(target_os = "windows"))]
fn place_window_on_cursor_monitor_top_center(_window: &tauri::WebviewWindow) {}

fn parse_startup_job_from_args() -> Option<StartupJob> {
  let args: Vec<String> = std::env::args().collect();
  parse_startup_job_from_list(&args)
}

#[tauri::command]
fn take_startup_job(state: tauri::State<AppState>) -> Option<StartupJob> {
  if let Ok(mut guard) = state.startup_job.lock() {
    let job = guard.take();
    if let Some(found) = job.as_ref() {
      write_log(
        "INFO",
        &format!(
          "startup job consumed by frontend: {}{}",
          found.script,
          found
            .target_path
            .as_ref()
            .map(|target| format!(" target={target}"))
            .unwrap_or_default()
        ),
      );
    }
    return job;
  }
  None
}

#[tauri::command]
fn get_script_catalog() -> Result<Vec<ScriptMenuItem>, String> {
  load_script_catalog_from_file()
}

#[tauri::command]
fn create_script_item(label: String, script_name: String) -> Result<Vec<ScriptMenuItem>, String> {
  ensure_safe_script_name(&script_name)?;

  let script_name = script_name.trim().to_string();
  let display_label = if label.trim().is_empty() {
    script_name.clone()
  } else {
    label.trim().to_string()
  };

  let mut catalog = load_script_catalog_from_file()?;
  if catalog.iter().any(|item| item.script == script_name) {
    return Err(format!("脚本已存在: {script_name}"));
  }

  let scripts_dir = resolve_scripts_dir()?;
  let script_path = scripts_dir.join(&script_name);
  if script_path.exists() {
    return Err(format!("脚本文件已存在: {script_name}"));
  }

  std::fs::write(&script_path, script_template(&script_name))
    .map_err(|e| format!("创建脚本文件失败: {e}"))?;

  write_log("INFO", &format!("script created: {script_name}"));
  catalog.push(ScriptMenuItem {
    label: display_label,
    script: script_name,
  });
  save_script_catalog(&catalog)?;
  open_path_in_default_editor(&script_path)?;

  Ok(catalog)
}

#[tauri::command]
fn open_script_in_editor(script_name: String) -> Result<(), String> {
  ensure_safe_script_name(&script_name)?;
  let path = resolve_script_path(script_name.trim())?;
  open_path_in_default_editor(&path)
}

#[tauri::command]
fn delete_script_item(script_name: String) -> Result<Vec<ScriptMenuItem>, String> {
  ensure_safe_script_name(&script_name)?;
  let script_name = script_name.trim().to_string();

  let mut catalog = load_script_catalog_from_file()?;
  let old_len = catalog.len();
  catalog.retain(|item| item.script != script_name);
  if catalog.len() == old_len {
    return Err("脚本不存在，无法删除".to_string());
  }

  save_script_catalog(&catalog)?;

  let scripts_dir = resolve_scripts_dir()?;
  let script_path = scripts_dir.join(&script_name);
  if script_path.exists() {
    std::fs::remove_file(&script_path).map_err(|e| format!("删除脚本文件失败: {e}"))?;
  }

  write_log("INFO", &format!("script deleted: {script_name}"));
  Ok(catalog)
}

#[tauri::command]
fn get_system_integration_config() -> Result<SystemIntegrationConfig, String> {
  let config = load_or_init_app_config()?;
  Ok(config.system_integration)
}

#[tauri::command]
fn get_environment_configs() -> Result<Vec<EnvironmentConfig>, String> {
  let config = load_or_init_app_config()?;
  Ok(config.environments)
}

#[tauri::command]
fn get_idle_hide_seconds() -> Result<u32, String> {
  let config = load_or_init_app_config()?;
  Ok(config.idle_hide_seconds)
}

#[tauri::command]
fn set_idle_hide_seconds(idle_hide_seconds: u32) -> Result<u32, String> {
  let idle_hide_seconds = normalize_idle_hide_seconds(idle_hide_seconds)?;
  let mut config = load_or_init_app_config()?;
  config.idle_hide_seconds = idle_hide_seconds;
  save_app_config(&config)?;
  write_log("INFO", &format!("idle hide seconds updated: {idle_hide_seconds}"));
  Ok(idle_hide_seconds)
}

#[tauri::command]
fn set_environment_configs(environments: Vec<EnvironmentConfig>) -> Result<Vec<EnvironmentConfig>, String> {
  if environments.is_empty() {
    return Err("至少保留一个环境配置".to_string());
  }

  let mut cleaned = Vec::<EnvironmentConfig>::new();
  for item in environments {
    let name = item.name.trim().to_string();
    let executable_path = item.executable_path.trim().to_string();
    if name.is_empty() {
      continue;
    }
    cleaned.push(EnvironmentConfig {
      name,
      executable_path,
    });
  }

  if cleaned.is_empty() {
    return Err("环境配置不能为空".to_string());
  }

  let mut config = load_or_init_app_config()?;
  config.environments = cleaned.clone();
  save_app_config(&config)?;

  write_log("INFO", &format!("environments saved ({} entries)", cleaned.len()));
  Ok(cleaned)
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
  let url = url.trim().to_string();
  if !url.starts_with("https://") && !url.starts_with("http://") {
    return Err("只允许打开 http/https 链接".to_string());
  }

  #[cfg(target_os = "windows")]
  {
    Command::new("cmd")
      .args(["/c", "start", "", &url])
      .spawn()
      .map_err(|e| format!("打开链接失败: {e}"))?;
    return Ok(());
  }

  #[cfg(not(target_os = "windows"))]
  {
    Err("当前平台暂不支持打开链接".to_string())
  }
}

#[tauri::command]
fn open_configs_folder() -> Result<String, String> {
  let configs_dir = resolve_configs_dir()?;

  #[cfg(target_os = "windows")]
  {
    Command::new("explorer")
      .arg(&configs_dir)
      .spawn()
      .map_err(|e| format!("打开 configs 文件夹失败: {e}"))?;

    let path = configs_dir.to_string_lossy().to_string();
    write_log("INFO", &format!("configs folder opened: {path}"));
    return Ok(path);
  }

  #[cfg(not(target_os = "windows"))]
  {
    Err("当前平台暂不支持打开 configs 文件夹".to_string())
  }
}

#[tauri::command]
fn open_logs_folder() -> Result<String, String> {
  let logs_dir = resolve_logs_dir()
    .ok_or_else(|| "无法解析 logs 文件夹路径".to_string())?;

  #[cfg(target_os = "windows")]
  {
    Command::new("explorer")
      .arg(&logs_dir)
      .spawn()
      .map_err(|e| format!("打开 logs 文件夹失败: {e}"))?;

    let path = logs_dir.to_string_lossy().to_string();
    write_log("INFO", &format!("logs folder opened: {path}"));
    return Ok(path);
  }

  #[cfg(not(target_os = "windows"))]
  {
    Err("当前平台暂不支持打开 logs 文件夹".to_string())
  }
}

#[tauri::command]
fn write_app_log(level: String, message: String) {
  let level = match level.to_uppercase().as_str() {
    "INFO" | "WARN" | "ERROR" | "RUN" => level.to_uppercase(),
    _ => "INFO".to_string(),
  };
  write_log(&level, &message);
}

#[tauri::command]
fn pick_environment_executable() -> Result<Option<String>, String> {
  #[cfg(target_os = "windows")]
  {
    // Prefer selecting the bin directory (e.g. MinGW/bin); fallback to selecting a concrete executable.
    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
      return Ok(Some(folder.to_string_lossy().to_string()));
    }

    let picked = rfd::FileDialog::new().pick_file();
    return Ok(picked.map(|path| path.to_string_lossy().to_string()));
  }

  #[cfg(not(target_os = "windows"))]
  {
    Err("当前平台不支持该选择器".to_string())
  }
}

#[tauri::command]
fn set_system_integration_config(
  auto_start_enabled: bool,
  context_menu_enabled: bool,
) -> Result<SystemIntegrationConfig, String> {
  #[cfg(target_os = "windows")]
  {
    apply_auto_start(auto_start_enabled)?;
    apply_context_menu(context_menu_enabled)?;
  }

  #[cfg(not(target_os = "windows"))]
  {
    let _ = auto_start_enabled;
    let _ = context_menu_enabled;
    return Err("当前平台不支持系统集成设置".to_string());
  }

  let integration = SystemIntegrationConfig {
    auto_start_enabled,
    context_menu_enabled,
  };
  let mut config = load_or_init_app_config()?;
  config.system_integration = integration.clone();
  save_app_config(&config)?;

  write_log("INFO", &format!("system integration: auto_start={auto_start_enabled} context_menu={context_menu_enabled}"));
  Ok(integration)
}

#[tauri::command]
fn run_demo_script(
  window: tauri::Window,
  script_name: String,
  target_path: Option<String>,
) -> Result<ScriptRunResult, String> {
  ensure_safe_script_name(&script_name)?;

  let script_name = script_name.trim().to_string();
  let ext = Path::new(&script_name)
    .extension()
    .and_then(|s| s.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();

  let script_path = resolve_script_path(&script_name)?;

  write_log("RUN", &format!("script started: {script_name} (ext={ext})"));
  let run_one = |mut command: Command, launch_error_prefix: &str| -> Result<ScriptRunResult, String> {
    if let Some(target) = target_path.clone() {
      if !target.trim().is_empty() {
        command.arg(target);
      }
    }
    run_command_with_stream(&window, command, launch_error_prefix)
  };

  match ext.as_str() {
    "py" => {
      let python_exe = choose_python_executable();
      let mut command = Command::new(&python_exe);
      command
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .arg(Path::new(&script_path));
      run_one(command, "启动 Python 失败")
    }
    "js" | "mjs" | "cjs" => {
      let node_exe = choose_env_executable("NODE", "node");
      let mut command = Command::new(&node_exe);
      command.arg(Path::new(&script_path));
      run_one(command, "启动 Node 失败")
    }
    "java" => {
      let javac_exe = choose_java_compiler_executable();
      let java_exe = choose_java_runtime_executable();
      let (source, simple_class_name, main_class) = java_compile_source_details(&script_path)?;
      let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
      let compile_dir = std::env::temp_dir().join(format!("omniisle_java_{}", now));
      std::fs::create_dir_all(&compile_dir)
        .map_err(|e| format!("创建 Java 编译目录失败: {e}"))?;
      let source_path = compile_dir.join(format!("{}.java", simple_class_name));
      std::fs::write(&source_path, source)
        .map_err(|e| format!("写入临时 Java 源码失败: {e}"))?;

      let mut compile = Command::new(&javac_exe);
      compile
        .arg("-encoding")
        .arg("UTF-8")
        .arg("-d")
        .arg(&compile_dir)
        .arg(&source_path);

      let compile_result = run_command_with_stream(&window, compile, "调用 javac 编译失败")?;
      if !compile_result.success {
        let _ = std::fs::remove_dir_all(&compile_dir);
        return Ok(compile_result);
      }

      let mut run = Command::new(&java_exe);
      run.arg("-cp").arg(&compile_dir).arg(&main_class);
      if let Some(target) = target_path.clone() {
        if !target.trim().is_empty() {
          run.arg(target);
        }
      }
      let run_result = run_command_with_stream(&window, run, "启动 Java 失败");
      let _ = std::fs::remove_dir_all(&compile_dir);
      run_result
    }
    "c" | "cpp" | "cc" | "cxx" => {
      let is_cpp = ext == "cpp" || ext == "cc" || ext == "cxx";
      let compiler = if is_cpp {
        choose_cpp_executable()
      } else {
        choose_c_executable()
      };

      let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
      let stem = Path::new(&script_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("script");
      let binary_name = if cfg!(target_os = "windows") {
        format!("omniisle_{}_{}.exe", stem, now)
      } else {
        format!("omniisle_{}_{}", stem, now)
      };
      let binary_path = std::env::temp_dir().join(binary_name);

      let mut compile = Command::new(&compiler);
      apply_mingw_toolchain_env(&mut compile);
      compile
        .arg(Path::new(&script_path))
        .arg("-o")
        .arg(&binary_path);

      let compile_result = run_command_with_stream(&window, compile, "调用 GCC 编译失败")?;
      if !compile_result.success {
        return Ok(compile_result);
      }

      let mut run = Command::new(&binary_path);
      apply_mingw_runtime_env(&mut run);
      let run_result = run_one(run, "运行编译结果失败");
      let _ = std::fs::remove_file(&binary_path);
      run_result
    }
    _ => Err(format!("不支持的脚本类型: {}", script_name)),
  }
}

#[tauri::command]
fn sync_main_window_size(window: tauri::WebviewWindow, mode: String) {
    let (width, height) = match mode.as_str() {
        "collapsed" => (700.0, 80.0),
        "expanded" => (700.0, 380.0),
        "settings" => (700.0, 520.0),
        _ => (700.0, 520.0)
    };

    if let Ok(scale_factor) = window.scale_factor() {
        let physical_width = (width * scale_factor).round() as u32;
        let physical_height = (height * scale_factor).round() as u32;

        #[cfg(target_os = "windows")]
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                SetWindowPos(
                    hwnd.0 as _,
                    std::ptr::null_mut(),
                    0,
                    0,
                    physical_width as i32,
                    physical_height as i32,
                    SWP_NOZORDER | SWP_NOACTIVATE | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE,
                );
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = window.set_size(tauri::PhysicalSize::new(physical_width, physical_height));
        }
    }
}

fn show_main_window(app: &tauri::AppHandle) {
  if let Some(main_window) = app.get_webview_window("main") {
    place_window_on_cursor_monitor_top_center(&main_window);
    let _ = main_window.show();
    let _ = main_window.set_focus();
    let _ = main_window.emit("tray-show-island", ());
    write_log("INFO", "main window shown from tray");
  }
}

fn parse_startup_job_from_list(args: &[String]) -> Option<StartupJob> {
  if args.is_empty() {
    return None;
  }

  let mut script: Option<String> = None;
  let mut target_path: Option<String> = None;
  let mut i = 1usize;

  while i < args.len() {
    match args[i].as_str() {
      "--run-script" if i + 1 < args.len() => {
        script = Some(args[i + 1].clone());
        i += 2;
      }
      "--target" if i + 1 < args.len() => {
        target_path = Some(args[i + 1].clone());
        i += 2;
      }
      _ => {
        i += 1;
      }
    }
  }

  let job = script.map(|s| StartupJob {
    script: s,
    target_path,
  });

  if let Some(found) = job.as_ref() {
    write_log(
      "INFO",
      &format!(
        "startup job parsed from args: {}{}",
        found.script,
        found
          .target_path
          .as_ref()
          .map(|target| format!(" target={target}"))
          .unwrap_or_default()
      ),
    );
  }

  job
}

fn forward_startup_job(app: &tauri::AppHandle, startup_job: Option<StartupJob>) {
  if let Some(job) = startup_job {
    if let Some(state) = app.try_state::<AppState>() {
      if let Ok(mut guard) = state.startup_job.lock() {
        *guard = Some(job.clone());
      }
    }
    write_log("INFO", &format!("startup job forwarded to primary instance: {}", job.script));
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  purge_old_logs();
  write_log("INFO", "OmniIsle started");
  let startup_job = parse_startup_job_from_args();

  tauri::Builder::default()
    .manage(AppState {
      startup_job: Mutex::new(startup_job),
    })
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
        write_log("INFO", "main window close requested; hidden to tray");
      }
    })
    .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
      let startup_job = parse_startup_job_from_list(&argv);
      forward_startup_job(app, startup_job);
      show_main_window(app);
    }))
    .invoke_handler(tauri::generate_handler![
      run_demo_script,
      get_script_catalog,
      create_script_item,
      open_script_in_editor,
      delete_script_item,
      get_system_integration_config,
      set_system_integration_config,
      get_environment_configs,
      set_environment_configs,
      get_idle_hide_seconds,
      set_idle_hide_seconds,
      pick_environment_executable,
      open_url,
      open_configs_folder,
      open_logs_folder,
      write_app_log,
      take_startup_job,
      sync_main_window_size
    ])
    .setup(|app| {
      if let Some(main_window) = app.get_webview_window("main") {
        place_window_on_cursor_monitor_top_center(&main_window);
      }

      if let Some(state) = app.try_state::<AppState>() {
        if let Ok(guard) = state.startup_job.lock() {
          if guard.is_some() {
            show_main_window(app.handle());
          }
        }
      }

      let show_item = MenuItem::with_id(app, "tray_show", "显示软件", true, None::<&str>)?;
      let quit_item = MenuItem::with_id(app, "tray_quit", "退出软件", true, None::<&str>)?;
      let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

      let icon = app
        .default_window_icon()
        .ok_or("无法加载托盘图标")?
        .clone();

      let app_handle = app.handle().clone();
      TrayIconBuilder::with_id("main-tray")
        .menu(&tray_menu)
        .icon(icon)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |_tray, event| {
          if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
          } = event
          {
            show_main_window(&app_handle);
          }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
          "tray_show" => show_main_window(app),
          "tray_quit" => {
            write_log("INFO", "app exit from tray menu");
            app.exit(0);
          }
          _ => {}
        })
        .build(app)?;

      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
