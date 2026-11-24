"""Genera los íconos de Kuntur a partir del diseño original (ajustes v0.2.1).

Entrada: una imagen cuadrada con el ícono (un cuadrado redondeado) sobre un fondo oscuro, por ejemplo
`assets/kunturv2.jpg`. Salida, en `assets/`:
  - `kuntur-1024.png`: el cuadrado recortado, con el exterior transparente (archivo maestro).
  - `kuntur-64.png`: el de la barra de título.
  - `kuntur.ico`: 16 a 256 px (cada tamaño como PNG dentro del .ico), para el exe y los accesos directos.

Uso: python -I scripts/icono-desde-png.py assets/kunturv2.jpg
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def bounds_of_square(img: Image.Image) -> tuple[int, int, int, int]:
    """Caja del cuadrado: los píxeles claramente más claros que el fondo (el borde que brilla)."""
    gray = img.convert("L")
    # El fondo ronda 10-25; el borde del cuadrado pasa de 60.
    mask = gray.point(lambda v: 255 if v > 60 else 0)
    box = mask.getbbox()
    if box is None:
        raise SystemExit("no se encontró el cuadrado del ícono")
    return box


def rounded_mask(size: int, radius: float) -> Image.Image:
    """Máscara del cuadrado redondeado, con bordes suaves (se dibuja a 4x y se reduce)."""
    big = size * 4
    m = Image.new("L", (big, big), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, big - 1, big - 1), radius=radius * 4, fill=255)
    return m.resize((size, size), Image.LANCZOS)


def main() -> None:
    src = Path(sys.argv[1])
    out = Path(__file__).resolve().parent.parent / "assets"
    img = Image.open(src).convert("RGB")
    left, top, right, bottom = bounds_of_square(img)
    side = max(right - left, bottom - top)
    cx, cy = (left + right) / 2, (top + bottom) / 2
    # Un poco hacia adentro para que no quede el halo exterior.
    inset = side * 0.006
    half = side / 2 - inset
    square = img.crop((round(cx - half), round(cy - half), round(cx + half), round(cy + half)))
    master = square.resize((1024, 1024), Image.LANCZOS).convert("RGBA")
    # El radio de la esquina del diseño es ~18 % del lado.
    master.putalpha(rounded_mask(1024, 1024 * 0.18))
    master.save(out / "kuntur-1024.png")

    # Para los tamaños chicos, el trazo de neón se engrosa antes de reducir (si no, a 16 px casi no se ve).
    bold = master.filter(ImageFilter.MaxFilter(17))
    bold.putalpha(master.getchannel("A"))

    def at(size: int) -> Image.Image:
        base = bold if size <= 32 else master
        small = base.resize((size, size), Image.LANCZOS)
        # En los tamaños chicos el trazo fino se pierde: un poco de nitidez ayuda.
        if size <= 48:
            small = small.filter(ImageFilter.UnsharpMask(radius=1, percent=120, threshold=0))
        return small

    # La barra de título lo muestra a ~18 px: se usa el trazo grueso aunque el archivo sea de 64 px.
    bold.resize((64, 64), Image.LANCZOS).save(out / "kuntur-64.png")
    frames = [at(s) for s in ICO_SIZES]
    frames[-1].save(out / "kuntur.ico", format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])
    print("box", (left, top, right, bottom), "lado", side)
    for name in ("kuntur-1024.png", "kuntur-64.png", "kuntur.ico"):
        print(name, (out / name).stat().st_size, "bytes")


if __name__ == "__main__":
    main()
