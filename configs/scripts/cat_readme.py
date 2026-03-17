from __future__ import annotations

import sys
from pathlib import Path

SUPPORTED_EXTENSIONS = {
    ".txt",
    ".md",
    ".py",
    ".json",
    ".yaml",
    ".yml",
    ".ini",
    ".toml",
    ".csv",
    ".log",
}
MAX_LINES = 200
MAX_CHARS = 12000


def main() -> int:
    if len(sys.argv) < 2:
        print("请对文本文件右键运行此脚本。", file=sys.stderr)
        return 1

    target = Path(sys.argv[1]).expanduser().resolve()
    if not target.exists() or not target.is_file():
        print(f"目标不是有效文件: {target}", file=sys.stderr)
        return 1

    if target.suffix.lower() not in SUPPORTED_EXTENSIONS:
        print(f"暂不支持该文件类型: {target.suffix}", file=sys.stderr)
        print("支持类型: " + ", ".join(sorted(SUPPORTED_EXTENSIONS)), file=sys.stderr)
        return 1

    try:
        text = target.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        text = target.read_text(encoding="gbk", errors="replace")
    except Exception as exc:
        print(f"读取文件失败: {exc}", file=sys.stderr)
        return 1

    lines = text.splitlines()
    clipped_lines = lines[:MAX_LINES]
    clipped_text = "\n".join(clipped_lines)
    if len(clipped_text) > MAX_CHARS:
        clipped_text = clipped_text[:MAX_CHARS]

    print(f"文件: {target}")
    print(f"总行数: {len(lines)}")
    print("-" * 40)
    print(clipped_text)

    if len(lines) > MAX_LINES or len("\n".join(clipped_lines)) > len(clipped_text):
        print("-" * 40)
        print("内容过长，已截断显示。")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
