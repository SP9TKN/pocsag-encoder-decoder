"""Rysuje ikonę programu: pager z wyświetlaczem i fale radiowe. Uruchamiać z katalogu assets;
wynik: ikona.ico, ikona_256.png, ikona_64.rgba. Styl jak ikona dekodera (ucho): to samo tło i fale."""
import math
import sys
from PIL import Image, ImageDraw

R = 1024  # płótno robocze, zmniejszane z wygładzaniem


def gruba(d, pts, szer, kolor):
    """Linia stemplowana kołami: gładka, bez szczelin na załamaniach."""
    r = szer / 2
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        kroki = max(1, int(math.hypot(x1 - x0, y1 - y0)))
        for i in range(kroki + 1):
            x, y = x0 + (x1 - x0) * i / kroki, y0 + (y1 - y0) * i / kroki
            d.ellipse((x - r, y - r, x + r, y + r), fill=kolor)


# Tło: zaokrąglony kwadrat z pionowym gradientem, jak w dekoderze.
gora, dol = (24, 92, 112), (9, 40, 66)
grad = Image.new("RGBA", (R, R))
for y in range(R):
    t = y / (R - 1)
    grad.paste(tuple(int(a + (b - a) * t) for a, b in zip(gora, dol)) + (255,), (0, y, R, y + 1))
maska = Image.new("L", (R, R), 0)
ImageDraw.Draw(maska).rounded_rectangle((20, 20, R - 21, R - 21), radius=210, fill=255)
img = Image.new("RGBA", (R, R), (0, 0, 0, 0))
img.paste(grad, (0, 0), maska)
d = ImageDraw.Draw(img)

JASNY = (244, 248, 250, 255)
CIEMNY = (14, 58, 82, 255)
FALA = (92, 220, 190, 255)
LCD = (150, 214, 120, 255)
LCD_TEKST = (28, 70, 40, 255)

# Obudowa pagera: poziomy prostokąt w dolnej części, nad nim miejsce na fale.
X0, Y0, X1, Y1 = 150, 380, 874, 860
d.rounded_rectangle((X0, Y0, X1, Y1), radius=95, fill=JASNY)
# Antena: krótki występ na górnej krawędzi po prawej, z niego idą fale.
d.rounded_rectangle((690, 300, 790, 420), radius=34, fill=JASNY)
# Wyświetlacz z dwiema liniami „tekstu”.
d.rounded_rectangle((225, 450, 799, 690), radius=38, fill=CIEMNY)
d.rounded_rectangle((250, 475, 774, 665), radius=24, fill=LCD)
for y, dl in ((515, 430), (590, 300)):
    d.rounded_rectangle((290, y, 290 + dl, y + 42), radius=14, fill=LCD_TEKST)
# Trzy przyciski pod wyświetlaczem.
for x in (330, 512, 694):
    d.rounded_rectangle((x - 62, 735, x + 62, 800), radius=30, fill=CIEMNY)

# Fale radiowe od anteny w górę, jak w ikonie dekodera.
sr = (740, 300)
for promien in (80, 160, 240):
    luk = [(sr[0] + promien * math.cos(math.radians(a)), sr[1] + promien * math.sin(math.radians(a))) for a in range(-140, -39, 2)]
    gruba(d, luk, 46, FALA)

img.resize((256, 256), Image.LANCZOS).save("ikona_256.png")
img.resize((256, 256), Image.LANCZOS).save("ikona.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
with open("ikona_64.rgba", "wb") as f:
    f.write(img.resize((64, 64), Image.LANCZOS).convert("RGBA").tobytes())
# Podgląd w małych rozmiarach na jasnym tle.
pod = Image.new("RGBA", (760, 300), (235, 235, 235, 255))
x = 10
for s in (256, 64, 48, 32, 24, 16):
    m = img.resize((s, s), Image.LANCZOS)
    pod.alpha_composite(m, (x, 20))
    x += s + 14
if len(sys.argv) > 1:
    pod.save(sys.argv[1])
