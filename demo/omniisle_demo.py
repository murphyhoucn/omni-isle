import io
import json
import locale
import os
import queue
import subprocess
import sys
import threading
import tkinter as tk
from dataclasses import dataclass
from pathlib import Path


@dataclass
class IslandState:
    label: str
    bg: str


STATE_READY = IslandState("准备就绪", "#1f1f1f")
STATE_RUNNING = IslandState("处理中...", "#2f2f2f")
STATE_SUCCESS = IslandState("执行成功", "#0d7f46")
STATE_ERROR = IslandState("执行失败", "#9e1f2d")

DEFAULT_SCRIPT_ITEMS = [
    {"label": "运行成功脚本", "script": "mock_success.py"},
    {"label": "运行失败脚本(演示)", "script": "mock_error.py"},
]


class OmniIsleDemo:
    def __init__(self, root: tk.Tk) -> None:
        self.root = root
        self.root.title("OmniIsle Demo")
        self.root.overrideredirect(True)
        self.root.attributes("-topmost", True)
        self.root.configure(bg="#000000")

        self.collapsed_size = (240, 44)
        self.expanded_size = (540, 360)
        self.is_expanded = False

        self.state = STATE_READY
        self.logs: list[str] = []
        self.worker: threading.Thread | None = None
        self.event_queue: queue.Queue[tuple[str, str]] = queue.Queue()
        self.demo_dir = Path(__file__).parent
        self.script_items = self.load_script_items()

        self.container = tk.Frame(self.root, bg=self.state.bg, bd=0, highlightthickness=0)
        self.container.pack(fill="both", expand=True)

        self.header = tk.Frame(self.container, bg=self.state.bg)
        self.header.pack(fill="x", padx=10, pady=(8, 6))
        self.header.bind("<Button-1>", lambda _: self.toggle_expand())

        self.status_dot = tk.Canvas(self.header, width=12, height=12, bg=self.state.bg, highlightthickness=0)
        self.status_dot.create_oval(1, 1, 11, 11, fill="#f6f6f6", outline="")
        self.status_dot.pack(side="left")
        self.status_dot.bind("<Button-1>", lambda _: self.toggle_expand())

        self.title_label = tk.Label(
            self.header,
            text=self.state.label,
            fg="#f6f6f6",
            bg=self.state.bg,
            font=("Segoe UI", 10, "bold"),
            anchor="w",
        )
        self.title_label.pack(side="left", padx=(8, 0))
        self.title_label.bind("<Button-1>", lambda _: self.toggle_expand())

        self.btn_frame = tk.Frame(self.container, bg=self.state.bg)
        self.btn_frame.pack(fill="x", padx=10, pady=(0, 8))
        self.create_script_buttons()

        self.log_frame = tk.Frame(self.container, bg="#111111")
        self.log_text = tk.Text(
            self.log_frame,
            bg="#111111",
            fg="#dbdbdb",
            insertbackground="#dbdbdb",
            font=("Consolas", 10),
            wrap="word",
            bd=0,
            padx=10,
            pady=10,
        )
        self.log_text.pack(fill="both", expand=True)

        self.footer = tk.Frame(self.container, bg=self.state.bg)
        self.footer.pack(fill="x", padx=10, pady=(6, 8))

        self.hint = tk.Label(
            self.footer,
            text="点击顶部可收起/展开，拖动顶部可移动",
            fg="#d0d0d0",
            bg=self.state.bg,
            font=("Segoe UI", 9),
            anchor="w",
        )
        self.hint.pack(side="left")

        self.close_btn = tk.Button(
            self.footer,
            text="退出",
            command=self.root.destroy,
            relief="flat",
            bg="#f5f5f5",
            activebackground="#e7e7e7",
            cursor="hand2",
        )
        self.close_btn.pack(side="right")

        self._drag_start = (0, 0)
        self.header.bind("<ButtonPress-1>", self.start_drag)
        self.header.bind("<B1-Motion>", self.do_drag)

        self.apply_size(*self.collapsed_size)
        self.place_top_center()
        self.refresh_ui()
        self.root.after(100, self.poll_events)

    def load_script_items(self) -> list[dict[str, str]]:
        config_path = self.demo_dir / "scripts_config.json"
        try:
            if config_path.exists():
                payload = json.loads(config_path.read_text(encoding="utf-8"))
                items = payload.get("scripts", [])
                valid_items: list[dict[str, str]] = []
                for item in items:
                    label = str(item.get("label", "")).strip()
                    script = str(item.get("script", "")).strip()
                    if label and script:
                        valid_items.append({"label": label, "script": script})
                if valid_items:
                    return valid_items
        except Exception:
            pass
        return DEFAULT_SCRIPT_ITEMS

    def create_script_buttons(self) -> None:
        for child in self.btn_frame.winfo_children():
            child.destroy()

        for index, item in enumerate(self.script_items):
            button = tk.Button(
                self.btn_frame,
                text=item["label"],
                command=lambda s=item["script"]: self.run_script(s),
                relief="flat",
                bg="#f5f5f5",
                activebackground="#e7e7e7",
                cursor="hand2",
            )
            button.grid(row=index // 2, column=index % 2, padx=4, pady=4, sticky="ew")

        self.btn_frame.grid_columnconfigure(0, weight=1)
        self.btn_frame.grid_columnconfigure(1, weight=1)

    def start_drag(self, event: tk.Event) -> None:
        self._drag_start = (event.x_root, event.y_root)

    def do_drag(self, event: tk.Event) -> None:
        dx = event.x_root - self._drag_start[0]
        dy = event.y_root - self._drag_start[1]
        x = self.root.winfo_x() + dx
        y = self.root.winfo_y() + dy
        self.root.geometry(f"+{x}+{max(0, y)}")
        self._drag_start = (event.x_root, event.y_root)

    def place_top_center(self) -> None:
        self.root.update_idletasks()
        sw = self.root.winfo_screenwidth()
        x = (sw - self.root.winfo_width()) // 2
        self.root.geometry(f"+{x}+18")

    def apply_size(self, width: int, height: int) -> None:
        x = self.root.winfo_x() if self.root.winfo_x() > 0 else 100
        y = self.root.winfo_y() if self.root.winfo_y() >= 0 else 18
        self.root.geometry(f"{width}x{height}+{x}+{y}")

    def animate_size(self, target_w: int, target_h: int, steps: int = 12) -> None:
        self.root.update_idletasks()
        start_w = self.root.winfo_width()
        start_h = self.root.winfo_height()

        dw = (target_w - start_w) / steps
        dh = (target_h - start_h) / steps

        def step(i: int) -> None:
            if i > steps:
                self.apply_size(target_w, target_h)
                return
            self.apply_size(int(start_w + dw * i), int(start_h + dh * i))
            self.root.after(12, lambda: step(i + 1))

        step(1)

    def toggle_expand(self) -> None:
        self.is_expanded = not self.is_expanded
        if self.is_expanded:
            self.log_frame.pack(fill="both", expand=True, padx=10, pady=(0, 6))
            self.animate_size(*self.expanded_size)
        else:
            self.log_frame.pack_forget()
            self.animate_size(*self.collapsed_size)

    def set_state(self, state: IslandState) -> None:
        self.state = state
        self.refresh_ui()

    def refresh_ui(self) -> None:
        self.container.configure(bg=self.state.bg)
        self.header.configure(bg=self.state.bg)
        self.status_dot.configure(bg=self.state.bg)
        self.status_dot.delete("all")
        self.status_dot.create_oval(1, 1, 11, 11, fill="#f6f6f6", outline="")
        self.title_label.configure(text=self.state.label, bg=self.state.bg)
        self.btn_frame.configure(bg=self.state.bg)
        self.footer.configure(bg=self.state.bg)
        self.hint.configure(bg=self.state.bg)

    def append_log(self, line: str) -> None:
        self.logs.append(line)
        self.log_text.insert("end", line + "\n")
        self.log_text.see("end")

    def clear_log(self) -> None:
        self.logs = []
        self.log_text.delete("1.0", "end")

    def run_script(self, script_name: str) -> None:
        if self.worker and self.worker.is_alive():
            self.append_log("[warn] 当前已有任务在运行")
            return

        self.clear_log()
        self.set_state(STATE_RUNNING)
        self.append_log(f"[info] 准备执行: {script_name}")

        if not self.is_expanded:
            self.toggle_expand()

        script_path = self.demo_dir / "scripts" / script_name
        if not script_path.exists():
            self.set_state(STATE_ERROR)
            self.append_log(f"[error] 脚本不存在: {script_path}")
            return

        self.worker = threading.Thread(
            target=self._run_subprocess,
            args=(script_path,),
            daemon=True,
        )
        self.worker.start()

    def _run_subprocess(self, script_path: Path) -> None:
        python_exe = sys.executable
        creation_flags = 0
        preferred_encoding = locale.getpreferredencoding(False) or "utf-8"
        if os.name == "nt":
            creation_flags = subprocess.CREATE_NO_WINDOW

        try:
            process = subprocess.Popen(
                [python_exe, str(script_path)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                encoding=preferred_encoding,
                errors="replace",
                creationflags=creation_flags,
            )
        except Exception as exc:
            self.event_queue.put(("err", f"[fatal] 启动失败: {exc}"))
            self.event_queue.put(("done", "1"))
            return

        assert process.stdout is not None
        assert process.stderr is not None

        # Read stdout/stderr concurrently to avoid blocking when one stream fills up.
        def pump(stream: io.TextIOBase, event_type: str) -> None:
            for line in stream:
                self.event_queue.put((event_type, line.rstrip("\n")))

        out_thread = threading.Thread(target=pump, args=(process.stdout, "out"), daemon=True)
        err_thread = threading.Thread(target=pump, args=(process.stderr, "err"), daemon=True)
        out_thread.start()
        err_thread.start()

        out_thread.join()
        err_thread.join()

        code = process.wait()
        self.event_queue.put(("done", str(code)))

    def poll_events(self) -> None:
        while not self.event_queue.empty():
            event_type, payload = self.event_queue.get_nowait()
            if event_type == "out":
                self.append_log("[stdout] " + payload)
            elif event_type == "err":
                self.append_log("[stderr] " + payload)
            elif event_type == "done":
                code = int(payload)
                if code == 0:
                    self.set_state(STATE_SUCCESS)
                    self.append_log("[info] 任务结束，退出码 0")
                else:
                    self.set_state(STATE_ERROR)
                    self.append_log(f"[error] 任务失败，退出码 {code}")

        self.root.after(100, self.poll_events)


def main() -> None:
    root = tk.Tk()
    app = OmniIsleDemo(root)
    app.place_top_center()
    root.mainloop()


if __name__ == "__main__":
    main()
