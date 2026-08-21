from __future__ import annotations

import hashlib
import json
from pathlib import Path

from PIL import Image, ImageOps


ROOT = Path(__file__).resolve().parent
MASTER = ROOT / "knightframe-ui-hero-master.png"
BLACK = ROOT / "knightframe-ui-hero-black-transparent.png"
WHITE = ROOT / "knightframe-ui-hero-white-transparent.png"
MANIFEST = ROOT / "brand-assets.manifest.json"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def mask_from_white_ground() -> Image.Image:
    # Preserve the supplied 1254 x 1254 composition exactly. Only convert its
    # white ground into alpha; do not crop, resize, retouch or move pixels.
    source = Image.open(MASTER).convert("L")
    inverted = ImageOps.invert(source)
    return inverted.point(
        lambda value: 0 if value <= 24 else 255 if value >= 176 else round((value - 24) * 255 / 152)
    )


def transparent(mask: Image.Image, value: int) -> Image.Image:
    channel = Image.new("L", mask.size, value)
    return Image.merge("RGBA", (channel, channel, channel, mask))


def update_manifest() -> None:
    data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    data["brand_layers"] = {
        "ui_hero": {
            "master": MASTER.name,
            "master_sha256": digest(MASTER),
            "locked": True,
            "minimum_display_px": 64,
            "derivatives": [
                {"file": BLACK.name, "sha256": digest(BLACK), "bytes": BLACK.stat().st_size},
                {"file": WHITE.name, "sha256": digest(WHITE), "bytes": WHITE.stat().st_size},
            ],
        },
        "compact_mark": {
            "master": data["production_master"]["file"],
            "master_sha256": data["production_master"]["sha256"],
            "locked": True,
            "use": "ICO and compact marks; all existing derivatives remain frozen",
        },
    }
    MANIFEST.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    if not MASTER.is_file():
        raise FileNotFoundError(MASTER)
    mask = mask_from_white_ground()
    transparent(mask, 0).save(BLACK, format="PNG", optimize=True)
    transparent(mask, 255).save(WHITE, format="PNG", optimize=True)
    update_manifest()


if __name__ == "__main__":
    main()
