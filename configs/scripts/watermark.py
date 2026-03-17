from __future__ import annotations

import sys
from pathlib import Path

try:
	from PIL import Image
except ImportError as exc:
	raise SystemExit(
		"缺少依赖 Pillow，请先在 Python 环境中安装: pip install Pillow"
	) from exc


# =========================
# 可自行修改的水印配置
# =========================
WATERMARK_PATH = Path(r"C:\Users\cosmi\OneDrive\图片\murphy.png")
OUTPUT_SUFFIX = "_watermarked"
OUTPUT_FORMAT = None  # 例如: "PNG" / "JPEG"，None 表示沿用原图格式

# 水印相对原图宽度的缩放比例，例如 0.22 表示水印宽度占原图宽度 22%
WATERMARK_SCALE_RATIO = 0.22

# 透明度范围 0.0 ~ 1.0
WATERMARK_OPACITY = 0.35

# 可选位置: top_left / top_right / center / bottom_left / bottom_right
WATERMARK_POSITION = "bottom_right"

# 边距，相对于原图短边的比例
PADDING_RATIO = 0.03

# 是否允许传入目录；若是目录，则批量处理该目录下所有图片
EXPAND_DIRECTORY_INPUT = True

SUPPORTED_IMAGE_EXTENSIONS = {
	".jpg",
	".jpeg",
	".png",
	".bmp",
	".webp",
	".tif",
	".tiff",
}


def collect_input_images(raw_args: list[str]) -> list[Path]:
	collected: list[Path] = []
	seen: set[Path] = set()

	for raw in raw_args:
		candidate = Path(raw).expanduser()
		if not candidate.exists():
			print(f"跳过不存在的路径: {candidate}")
			continue

		if candidate.is_dir():
			if not EXPAND_DIRECTORY_INPUT:
				print(f"跳过目录（已关闭目录展开）: {candidate}")
				continue
			for child in sorted(candidate.iterdir()):
				if child.is_file() and child.suffix.lower() in SUPPORTED_IMAGE_EXTENSIONS:
					resolved = child.resolve()
					if resolved not in seen:
						seen.add(resolved)
						collected.append(resolved)
			continue

		if candidate.suffix.lower() not in SUPPORTED_IMAGE_EXTENSIONS:
			print(f"跳过非图片文件: {candidate}")
			continue

		resolved = candidate.resolve()
		if resolved not in seen:
			seen.add(resolved)
			collected.append(resolved)

	return collected


def load_watermark() -> Image.Image:
	if not WATERMARK_PATH.exists():
		raise FileNotFoundError(f"未找到水印文件: {WATERMARK_PATH}")
	return Image.open(WATERMARK_PATH).convert("RGBA")


def resize_watermark(base_image: Image.Image, watermark: Image.Image) -> Image.Image:
	target_width = max(1, int(base_image.width * WATERMARK_SCALE_RATIO))
	scale = target_width / max(1, watermark.width)
	target_height = max(1, int(watermark.height * scale))
	return watermark.resize((target_width, target_height), Image.LANCZOS)


def apply_opacity(watermark: Image.Image) -> Image.Image:
	alpha = watermark.getchannel("A")
	alpha = alpha.point(lambda px: int(px * max(0.0, min(1.0, WATERMARK_OPACITY))))
	watermark = watermark.copy()
	watermark.putalpha(alpha)
	return watermark


def compute_position(base_image: Image.Image, watermark: Image.Image) -> tuple[int, int]:
	padding = max(4, int(min(base_image.width, base_image.height) * PADDING_RATIO))

	if WATERMARK_POSITION == "top_left":
		return (padding, padding)
	if WATERMARK_POSITION == "top_right":
		return (base_image.width - watermark.width - padding, padding)
	if WATERMARK_POSITION == "center":
		return (
			(base_image.width - watermark.width) // 2,
			(base_image.height - watermark.height) // 2,
		)
	if WATERMARK_POSITION == "bottom_left":
		return (padding, base_image.height - watermark.height - padding)

	return (
		base_image.width - watermark.width - padding,
		base_image.height - watermark.height - padding,
	)


def build_output_path(image_path: Path) -> Path:
	return image_path.with_name(f"{image_path.stem}{OUTPUT_SUFFIX}{image_path.suffix}")


def save_image(image: Image.Image, output_path: Path, original_ext: str) -> None:
	save_kwargs = {}
	image_to_save = image

	chosen_format = OUTPUT_FORMAT
	if chosen_format is None and original_ext.lower() in {".jpg", ".jpeg"}:
		image_to_save = image.convert("RGB")
		save_kwargs["quality"] = 95
		save_kwargs["subsampling"] = 0

	if chosen_format is not None:
		save_kwargs["format"] = chosen_format

	image_to_save.save(output_path, **save_kwargs)


def add_watermark_to_image(image_path: Path, watermark_source: Image.Image) -> None:
	with Image.open(image_path).convert("RGBA") as base_image:
		resized = resize_watermark(base_image, watermark_source)
		prepared = apply_opacity(resized)
		position = compute_position(base_image, prepared)

		canvas = base_image.copy()
		canvas.alpha_composite(prepared, dest=position)

		output_path = build_output_path(image_path)
		save_image(canvas, output_path, image_path.suffix)
		print(f"已输出: {output_path}")


def main() -> int:
	targets = collect_input_images(sys.argv[1:])
	if not targets:
		print("未找到可处理的图片。请右击图片调用，或手动传入一个/多个图片路径。")
		return 1

	print(f"水印文件: {WATERMARK_PATH}")
	print(
		f"配置: position={WATERMARK_POSITION}, opacity={WATERMARK_OPACITY}, scale={WATERMARK_SCALE_RATIO}, padding_ratio={PADDING_RATIO}"
	)

	try:
		watermark_source = load_watermark()
	except Exception as exc:
		print(f"加载水印失败: {exc}")
		return 1

	success_count = 0
	for image_path in targets:
		print(f"处理中: {image_path}")
		try:
			add_watermark_to_image(image_path, watermark_source)
			success_count += 1
		except Exception as exc:
			print(f"处理失败: {image_path} -> {exc}")

	print(f"完成: {success_count}/{len(targets)} 张图片已生成水印输出")
	return 0 if success_count == len(targets) else 1


if __name__ == "__main__":
	raise SystemExit(main())
