from __future__ import annotations

import io
import json
import hashlib
import struct
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageEnhance, ImageFilter, ImageOps


ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "knightframe-primary-master.png"
BACKUP = ROOT / "knightframe-primary-backup-approved.png"
HARD_ALT = ROOT / "knightframe-primary-hard-alt.png"
CANVAS_SIZE = 1024
ICON_SIZES = (16, 20, 24, 28, 32, 48, 64, 96, 128, 256)


def source_mask() -> Image.Image:
    """Turn the approved master's white ground into alpha without redrawing it."""
    source = Image.open(SOURCE).convert("L")
    # v5 is essentially black on white, with a soft generated edge.  Mapping
    # only the edge interval preserves antialiasing while removing white noise.
    inverted = ImageOps.invert(source)
    return inverted.point(
        lambda value: 0 if value <= 24 else 255 if value >= 176 else round((value - 24) * 255 / 152)
    )


def colored_transparent(mask: Image.Image, value: int) -> Image.Image:
    color = Image.new("L", mask.size, value)
    return Image.merge("RGBA", (color, color, color, mask))


def fitted_mask(mask: Image.Image, size: int = CANVAS_SIZE) -> Image.Image:
    """Fit the unchanged primary pose into a Windows-icon-safe square."""
    bbox = mask.getbbox()
    if bbox is None:
        raise ValueError("the primary master contains no foreground")
    subject = mask.crop(bbox)
    target = round(size * 0.88)
    scale = min(target / subject.width, target / subject.height)
    fitted = subject.resize(
        (round(subject.width * scale), round(subject.height * scale)),
        Image.Resampling.LANCZOS,
    )
    canvas = Image.new("L", (size, size), 0)
    x = (size - fitted.width) // 2
    y = (size - fitted.height) // 2
    canvas.paste(fitted, (x, y))
    return canvas


def opaque_icon(mask: Image.Image, foreground: int, background: int) -> Image.Image:
    background_image = Image.new("RGB", mask.size, (background,) * 3)
    foreground_image = Image.new("RGB", mask.size, (foreground,) * 3)
    background_image.paste(foreground_image, mask=mask)
    return background_image


def small_frame(mask: Image.Image, size: int, foreground: int, background: int) -> Image.Image:
    """Optically tune only 16–48 px frames; 64 px+ are direct derivatives."""
    oversample = 8 if size <= 32 else 4
    working_size = size * oversample
    bbox = mask.getbbox()
    if bbox is None:
        raise ValueError("v5 contains no foreground")
    subject = mask.crop(bbox)
    occupancy = 0.96 if size <= 32 else 0.92 if size <= 48 else 0.88
    target = round(working_size * occupancy)
    scale = min(target / subject.width, target / subject.height)
    high = subject.resize(
        (round(subject.width * scale), round(subject.height * scale)),
        Image.Resampling.LANCZOS,
    )
    placed = Image.new("L", (working_size, working_size), 0)
    placed.paste(high, ((working_size - high.width) // 2, (working_size - high.height) // 2))
    high = placed

    # Slightly increase local contrast only below 64 px. Keep every contour and
    # the pose from the approved master.
    if size <= 48:
        # Erode by a fraction of one final pixel.  This opens the existing
        # bridle, rider-leg and foreleg gaps; it does not redraw or move them.
        high = high.filter(ImageFilter.MinFilter(3))
        high = ImageEnhance.Contrast(high).enhance(1.15)
    low = high.resize((size, size), Image.Resampling.LANCZOS)
    if size <= 48:
        low = ImageEnhance.Contrast(low).enhance(1.18 if size <= 32 else 1.08)
    return opaque_icon(low, foreground, background).convert("RGBA")


def save_ico(mask: Image.Image, path: Path, foreground: int, background: int) -> None:
    payloads: list[bytes] = []
    for size in ICON_SIZES:
        buffer = io.BytesIO()
        small_frame(mask, size, foreground, background).save(buffer, format="PNG", optimize=True)
        payloads.append(buffer.getvalue())

    offset = 6 + 16 * len(payloads)
    with path.open("wb") as output:
        output.write(struct.pack("<HHH", 0, 1, len(payloads)))
        for size, payload in zip(ICON_SIZES, payloads):
            output.write(
                struct.pack(
                    "<BBBBHHII",
                    0 if size == 256 else size,
                    0 if size == 256 else size,
                    0,
                    0,
                    1,
                    32,
                    len(payload),
                    offset,
                )
            )
            offset += len(payload)
        for payload in payloads:
            output.write(payload)


def validation_sheet(mask: Image.Image) -> Image.Image:
    zoom = 8
    padding = 18
    label_height = 22
    cell_width = 64 * zoom + padding * 2
    cell_height = 64 * zoom + label_height + padding * 2
    sheet = Image.new("RGB", (cell_width * 2, cell_height * len(ICON_SIZES[:7])), "#707070")
    draw = ImageDraw.Draw(sheet)

    for row, size in enumerate(ICON_SIZES[:7]):
        for column, (foreground, background, label) in enumerate(
            ((0, 255, "light"), (255, 0, "dark"))
        ):
            frame = small_frame(mask, size, foreground, background).convert("RGB")
            enlarged = frame.resize((size * zoom, size * zoom), Image.Resampling.NEAREST)
            panel = Image.new("RGB", (64 * zoom, 64 * zoom), (background,) * 3)
            panel.paste(enlarged, ((panel.width - enlarged.width) // 2, (panel.height - enlarged.height) // 2))
            x = column * cell_width + padding
            y = row * cell_height + padding + label_height
            sheet.paste(panel, (x, y))
            draw.text((x, row * cell_height + padding), f"{size}px / {label}", fill="white")
    return sheet


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def write_manifest() -> None:
    shipping = (
        "knightframe-primary-black-transparent.png",
        "knightframe-primary-white-transparent.png",
        "knightframe-app-light-1024.png",
        "knightframe-app-dark-1024.png",
        "knightframe-app-light.ico",
        "knightframe-app-dark.ico",
        "knightframe-small-size-validation.png",
    )
    previous_layers = None
    manifest_path = ROOT / "brand-assets.manifest.json"
    if manifest_path.is_file():
        previous_layers = json.loads(manifest_path.read_text(encoding="utf-8")).get("brand_layers")
    payload = {
        "schema": 1,
        "production_master": {
            "file": SOURCE.name,
            "sha256": digest(SOURCE),
            "locked": True,
        },
        "approved_backup": {
            "file": BACKUP.name,
            "sha256": digest(BACKUP),
            "locked": True,
        },
        "reference_only": {
            "file": HARD_ALT.name,
            "sha256": digest(HARD_ALT),
            "locked": True,
            "shipping_input": False,
        },
        "optical_sizes": [16, 20, 24, 28, 32, 48],
        "direct_master_sizes": [64, 96, 128, 256, 1024],
        "ico_sizes": list(ICON_SIZES),
        "shipping": [
            {"file": name, "bytes": (ROOT / name).stat().st_size, "sha256": digest(ROOT / name)}
            for name in shipping
        ],
    }
    if previous_layers is not None:
        payload["brand_layers"] = previous_layers
    manifest_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


def main() -> None:
    if not SOURCE.is_file():
        raise FileNotFoundError(f"the approved primary master is missing: {SOURCE}")
    if not BACKUP.is_file():
        raise FileNotFoundError(f"the approved backup master is missing: {BACKUP}")
    if not HARD_ALT.is_file():
        raise FileNotFoundError(f"the reference-only hard alternative is missing: {HARD_ALT}")

    mask = source_mask()
    colored_transparent(mask, 0).save(
        ROOT / "knightframe-primary-black-transparent.png", format="PNG", optimize=True
    )
    colored_transparent(mask, 255).save(
        ROOT / "knightframe-primary-white-transparent.png", format="PNG", optimize=True
    )

    app_mask = fitted_mask(mask)
    opaque_icon(app_mask, 0, 255).save(
        ROOT / "knightframe-app-light-1024.png", format="PNG", optimize=True
    )
    opaque_icon(app_mask, 255, 0).save(
        ROOT / "knightframe-app-dark-1024.png", format="PNG", optimize=True
    )
    save_ico(app_mask, ROOT / "knightframe-app-light.ico", 0, 255)
    save_ico(app_mask, ROOT / "knightframe-app-dark.ico", 255, 0)

    preview_root = ROOT / "validation"
    preview_root.mkdir(exist_ok=True)
    for size in ICON_SIZES[:7]:
        small_frame(app_mask, size, 0, 255).save(
            preview_root / f"knightframe-light-{size}.png", format="PNG", optimize=True
        )
        small_frame(app_mask, size, 255, 0).save(
            preview_root / f"knightframe-dark-{size}.png", format="PNG", optimize=True
        )
    validation_sheet(app_mask).save(
        ROOT / "knightframe-small-size-validation.png", format="PNG", optimize=True
    )
    write_manifest()


if __name__ == "__main__":
    main()
