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
use windows_sys::Win32::Foundation::POINT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
  GetMonitorInfoW, MonitorFromPoint, MONITOR_DEFAULTTONEAREST, MONITORINFO,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

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

fn resolve_script_path(script_name: &str) -> Result<PathBuf, String> {
  let cwd = std::env::current_dir().map_err(|e| format!("无法读取当前目录: {e}"))?;
  let candidate_roots = vec![cwd.clone(), cwd.join(".."), cwd.join("..").join("..")];

  for root in candidate_roots {
    let candidate = root.join("demo").join("scripts").join(script_name);
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
    let candidate = root.join("demo").join("scripts_config.json");
    if candidate.exists() {
      return Ok(candidate);
    }
  }

  Err("未找到脚本配置文件 demo/scripts_config.json".to_string())
}

fn choose_python_executable() -> String {
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
    return Err("脚本配置为空，请在 demo/scripts_config.json 中添加 scripts 项".to_string());
  }

  Ok(items)
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
      take_startup_job
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
