"""Create the UI-only sword-gate derivative without changing the approved source art.

The approved PNG contains a nearly transparent full-canvas matte (mostly alpha=2).
That matte becomes a visible rectangle when WebView glow/filter effects are applied.
Only alpha is normalized here; RGB pixels, geometry, crop, and proportions are kept.
"""

from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "public" / "brand" / "knightframe-sword-gate-white.png"
OUTPUT = ROOT / "public" / "brand" / "knightframe-sword-gate-ui.png"


def normalize_alpha(alpha: int) -> int:
    if alpha <= 24:
        return 0
    return min(255, round((alpha - 24) * 255 / 201))


def main() -> None:
    image = Image.open(SOURCE).convert("RGBA")
    alpha = image.getchannel("A").point(normalize_alpha)
    image.putalpha(alpha)
    image.save(OUTPUT, optimize=True)


if __name__ == "__main__":
    main()
