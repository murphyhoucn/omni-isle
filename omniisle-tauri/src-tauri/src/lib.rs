use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
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

#[derive(Deserialize)]
struct ScriptConfigFile {
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
  ]
}

fn default_app_config() -> AppConfigFile {
  AppConfigFile {
    system_integration: default_system_integration(),
    environments: default_environments(),
  }
}

fn resolve_script_path(script_name: &str) -> Result<PathBuf, String> {
  let cwd = std::env::current_dir().map_err(|e| format!("无法读取当前目录: {e}"))?;
  let candidate_roots = vec![cwd.clone(), cwd.join(".."), cwd.join("..").join("..")];

  for root in candidate_roots {
    let candidate = root.join("configs").join("scripts").join(script_name);
    if candidate.exists() {
      return Ok(candidate);
    }
  }

  Err(format!("未找到脚本文件: {script_name}"))
}

fn resolve_script_config_path() -> Result<PathBuf, String> {
  let cwd = std::env::current_dir().map_err(|e| format!("无法读取当前目录: {e}"))?;
  let candidate_roots = vec![cwd.clone(), cwd.join(".."), cwd.join("..").join("..")];

  for root in candidate_roots {
    let candidate = root.join("configs").join("scripts_config.json");
    if candidate.exists() {
      return Ok(candidate);
    }
  }

  Err("未找到脚本配置文件 configs/scripts_config.json".to_string())
}

fn resolve_configs_dir() -> Result<PathBuf, String> {
  let cwd = std::env::current_dir().map_err(|e| format!("无法读取当前目录: {e}"))?;
  let candidate_roots = vec![cwd.clone(), cwd.join(".."), cwd.join("..").join("..")];

  for root in candidate_roots {
    let candidate = root.join("configs");
    if candidate.exists() && candidate.is_dir() {
      return Ok(candidate);
    }
  }

  Err("未找到 configs 目录".to_string())
}

fn resolve_app_config_path() -> Result<PathBuf, String> {
  Ok(resolve_configs_dir()?.join("app_configs.json"))
}

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

  if items.is_empty() {
    return Err("脚本配置为空，请在 configs/scripts_config.json 中添加 scripts 项".to_string());
  }

  Ok(items)
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
  if let Ok(config) = load_or_init_app_config() {
    if let Some(found) = config
      .environments
      .iter()
      .find(|item| item.name.trim().eq_ignore_ascii_case("PYTHON"))
    {
      let configured = found.executable_path.trim();
      if !configured.is_empty() {
        return configured.to_string();
      }
    }
  }

  if let Ok(custom) = std::env::var("OMNIISLE_PYTHON") {
    if !custom.trim().is_empty() {
      return custom;
    }
  }
  if cfg!(target_os = "windows") {
    return "python".to_string();
  }
  "python3".to_string()
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

  script.map(|s| StartupJob {
    script: s,
    target_path,
  })
}

#[tauri::command]
fn take_startup_job(state: tauri::State<AppState>) -> Option<StartupJob> {
  if let Ok(mut guard) = state.startup_job.lock() {
    return guard.take();
  }
  None
}

#[tauri::command]
fn get_script_catalog() -> Result<Vec<ScriptMenuItem>, String> {
  load_script_catalog_from_file()
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
fn pick_environment_executable() -> Result<Option<String>, String> {
  #[cfg(target_os = "windows")]
  {
    let picked = rfd::FileDialog::new()
      .add_filter("Executable", &["exe"])
      .pick_file();
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

  Ok(integration)
}

#[tauri::command]
fn run_demo_script(
  window: tauri::Window,
  script_name: String,
  target_path: Option<String>,
) -> Result<ScriptRunResult, String> {
  if script_name.contains('/') || script_name.contains('\\') {
    return Err("脚本名不能包含路径分隔符".to_string());
  }

  let script_path = resolve_script_path(&script_name)?;
  let python_exe = choose_python_executable();

  let mut command = Command::new(&python_exe);
  command
    .env("PYTHONIOENCODING", "utf-8")
    .env("PYTHONUTF8", "1")
    .arg(Path::new(&script_path));

  if let Some(target) = target_path {
    if !target.trim().is_empty() {
      command.arg(target);
    }
  }

  let mut child = command
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|e| {
      format!(
        "启动 Python 失败: {e}。请确认已安装 Python，或设置环境变量 OMNIISLE_PYTHON 为可执行路径。"
      )
    })?;

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
    .map_err(|e| format!("等待脚本结束失败: {e}"))?;

  let exit_code = status.code().unwrap_or(-1);

  Ok(ScriptRunResult {
    success: status.success(),
    exit_code,
    stdout: stdout_all,
    stderr: stderr_all,
  })
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let startup_job = parse_startup_job_from_args();

  tauri::Builder::default()
    .manage(AppState {
      startup_job: Mutex::new(startup_job),
    })
    .invoke_handler(tauri::generate_handler![
      run_demo_script,
      get_script_catalog,
      get_system_integration_config,
      set_system_integration_config,
      get_environment_configs,
      set_environment_configs,
      pick_environment_executable,
      open_url,
      take_startup_job,
      sync_main_window_size
    ])
    .setup(|app| {
      if let Some(main_window) = app.get_webview_window("main") {
        place_window_on_cursor_monitor_top_center(&main_window);
      }

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
