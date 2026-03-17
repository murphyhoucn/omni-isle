import sys
import time


def main() -> None:
    print("开始处理图片批次 B")
    time.sleep(0.6)
    print("读取输入文件 metadata")
    time.sleep(0.6)
    print("检测到参数缺失: --watermark-text", file=sys.stderr)
    time.sleep(0.4)
    print("任务已中止: 脚本参数不完整，无法继续", file=sys.stderr)
    sys.exit(1)


if __name__ == "__main__":
    main()
