#!/usr/bin/env python3
"""Shared, dependency-free primitives for the world-gen tooling.

Pure Python stdlib only (struct + zlib), matching `assets/gen_textures.py`, so
the pipeline runs anywhere with no install step. Provides:

  - PNG read/write (our writer always uses filter 0, so the reader is trivial),
  - value noise + fractal Brownian motion (fbm) for terrain and texture detail,
  - a small deterministic RNG for placing craters, lakes and vents,
  - colour helpers, and a compact 5x7 bitmap font for baking labels onto previews.

Nothing here is part of the deterministic simulation: these are presentation /
asset tools (see CLAUDE.md, "Two worlds, one wall").
"""

import math
import os
import struct
import zlib


# --------------------------------------------------------------------------- #
# skirmish scenario sites
# --------------------------------------------------------------------------- #
def scenario_sites(path=None):
    """Parse the skirmish `.map` for the sites worldgen must respect.

    Returns `(spawns, resources)`: lists of world `(x, z)` floats for every
    `hq` and every `ore`/`carbon` line. Worldgen keeps terrain features off
    these sites and levels the ground there, so the parser is the single
    source of truth - move a base or a cluster in the map file and the next
    bake protects the new spot.
    """
    if path is None:
        path = os.path.join(os.path.dirname(__file__), "..", "maps",
                            "crossfire_basin.map")
    spawns, resources = [], []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            parts = line.split()
            if parts[0] == "hq":
                spawns.append((float(parts[2]), float(parts[3])))
            elif parts[0] in ("ore", "carbon"):
                resources.append((float(parts[1]), float(parts[2])))
    return spawns, resources


# --------------------------------------------------------------------------- #
# PNG io
# --------------------------------------------------------------------------- #
def write_png(path, w, h, rgb, alpha=False):
    """Write an 8-bit PNG. `rgb` is a bytes-like of w*h*(4 if alpha else 3)."""
    channels = 4 if alpha else 3
    color_type = 6 if alpha else 2
    stride = w * channels
    raw = bytearray()
    for y in range(h):
        raw.append(0)  # filter: none (keeps our reader trivial)
        raw += rgb[y * stride:(y + 1) * stride]

    def chunk(typ, data):
        return (struct.pack(">I", len(data)) + typ + data
                + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF))

    out = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, color_type, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
           + chunk(b"IEND", b""))
    with open(path, "wb") as f:
        f.write(out)


def read_png(path):
    """Read an 8-bit PNG written by `write_png` (filter 0 only).

    Returns (w, h, channels, bytearray). Good enough for our own tiles; not a
    general PNG decoder.
    """
    with open(path, "rb") as f:
        data = f.read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos = 8
    w = h = channels = 0
    idat = bytearray()
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        typ = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if typ == b"IHDR":
            w, h, _bd, ct = struct.unpack(">IIBB", body[:10])
            channels = {2: 3, 6: 4, 0: 1}[ct]
        elif typ == b"IDAT":
            idat += body
        elif typ == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    stride = w * channels
    out = bytearray(stride * h)
    for y in range(h):
        src = y * (stride + 1)
        assert raw[src] == 0, "read_png only supports filter 0"
        out[y * stride:(y + 1) * stride] = raw[src + 1:src + 1 + stride]
    return w, h, channels, out


# --------------------------------------------------------------------------- #
# deterministic noise + rng
# --------------------------------------------------------------------------- #
def _hash2(ix, iy, seed):
    n = (ix * 374761393 + iy * 668265263 + seed * 362437) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def value_noise(x, y, seed):
    """Smooth 2D value noise in [0, 1)."""
    ix, iy = math.floor(x), math.floor(y)
    fx, fy = x - ix, y - iy
    a = _hash2(ix, iy, seed)
    b = _hash2(ix + 1, iy, seed)
    c = _hash2(ix, iy + 1, seed)
    d = _hash2(ix + 1, iy + 1, seed)
    ux = fx * fx * (3 - 2 * fx)
    uy = fy * fy * (3 - 2 * fy)
    top = a + (b - a) * ux
    bot = c + (d - c) * ux
    return top + (bot - top) * uy


def fbm(x, y, seed, octaves=5, lacunarity=2.0, gain=0.5):
    """Fractal Brownian motion in roughly [0, 1]."""
    total = 0.0
    amp = 0.5
    freq = 1.0
    norm = 0.0
    for o in range(octaves):
        total += value_noise(x * freq, y * freq, seed + o * 101) * amp
        norm += amp
        freq *= lacunarity
        amp *= gain
    return total / norm if norm else 0.0


def ridged(x, y, seed, octaves=5):
    """Ridged multifractal in [0, 1]: sharp crests, good for mountains/grooves."""
    total = 0.0
    amp = 0.5
    freq = 1.0
    norm = 0.0
    for o in range(octaves):
        n = value_noise(x * freq, y * freq, seed + o * 71)
        n = 1.0 - abs(2.0 * n - 1.0)
        total += n * n * amp
        norm += amp
        freq *= 2.0
        amp *= 0.5
    return total / norm if norm else 0.0


class Rng:
    """Tiny deterministic LCG (numerical-recipes constants)."""

    def __init__(self, seed):
        self.state = (seed ^ 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF

    def next_u32(self):
        self.state = (self.state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return (self.state >> 32) & 0xFFFFFFFF

    def rand(self):
        """Float in [0, 1)."""
        return self.next_u32() / 4294967296.0

    def uniform(self, lo, hi):
        return lo + (hi - lo) * self.rand()

    def randint(self, lo, hi):
        return lo + int(self.rand() * (hi - lo + 1))


# --------------------------------------------------------------------------- #
# colour helpers (all operate on 3-tuples of 0..255 ints unless noted)
# --------------------------------------------------------------------------- #
def clamp(v, lo, hi):
    return lo if v < lo else hi if v > hi else v


def clamp8(v):
    return int(clamp(v, 0, 255))


def lerp(a, b, t):
    return a + (b - a) * t


def lerp3(c0, c1, t):
    t = clamp(t, 0.0, 1.0)
    return (clamp8(lerp(c0[0], c1[0], t)),
            clamp8(lerp(c0[1], c1[1], t)),
            clamp8(lerp(c0[2], c1[2], t)))


def scale3(c, s):
    return (clamp8(c[0] * s), clamp8(c[1] * s), clamp8(c[2] * s))


def add3(c, d):
    return (clamp8(c[0] + d[0]), clamp8(c[1] + d[1]), clamp8(c[2] + d[2]))


def mul3(c, d):
    """Per-channel multiply where d is a 0..1-ish tuple (a tint)."""
    return (clamp8(c[0] * d[0]), clamp8(c[1] * d[1]), clamp8(c[2] * d[2]))


# --------------------------------------------------------------------------- #
# framebuffer
# --------------------------------------------------------------------------- #
class Frame:
    """A simple RGB framebuffer with the few raster ops the renderer needs."""

    def __init__(self, w, h, bg=(0, 0, 0)):
        self.w = w
        self.h = h
        self.buf = bytearray(w * h * 3)
        if bg != (0, 0, 0):
            for i in range(w * h):
                self.buf[i * 3:i * 3 + 3] = bytes(bg)

    def put(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            i = (y * self.w + x) * 3
            self.buf[i] = c[0]
            self.buf[i + 1] = c[1]
            self.buf[i + 2] = c[2]

    def get(self, x, y):
        i = (y * self.w + x) * 3
        return (self.buf[i], self.buf[i + 1], self.buf[i + 2])

    def blend(self, x, y, c, a):
        """Alpha-blend colour `c` over the pixel with coverage `a` in [0, 1]."""
        if not (0 <= x < self.w and 0 <= y < self.h) or a <= 0:
            return
        i = (y * self.w + x) * 3
        ia = 1.0 - a
        self.buf[i] = clamp8(self.buf[i] * ia + c[0] * a)
        self.buf[i + 1] = clamp8(self.buf[i + 1] * ia + c[1] * a)
        self.buf[i + 2] = clamp8(self.buf[i + 2] * ia + c[2] * a)

    def save(self, path):
        write_png(path, self.w, self.h, self.buf, alpha=False)

    def downscaled(self, k):
        """Return a new Frame shrunk by integer factor `k` (box average)."""
        nw, nh = self.w // k, self.h // k
        out = Frame(nw, nh)
        n = k * k
        for y in range(nh):
            for x in range(nw):
                r = g = b = 0
                for dy in range(k):
                    base = ((y * k + dy) * self.w + x * k) * 3
                    for dx in range(k):
                        i = base + dx * 3
                        r += self.buf[i]
                        g += self.buf[i + 1]
                        b += self.buf[i + 2]
                out.put(x, y, (r // n, g // n, b // n))
        return out

    def blit(self, src, ox, oy):
        """Copy another Frame onto this one at (ox, oy)."""
        for y in range(src.h):
            ty = oy + y
            if ty < 0 or ty >= self.h:
                continue
            for x in range(src.w):
                tx = ox + x
                if 0 <= tx < self.w:
                    di = (ty * self.w + tx) * 3
                    si = (y * src.w + x) * 3
                    self.buf[di:di + 3] = src.buf[si:si + 3]


# --------------------------------------------------------------------------- #
# 5x7 bitmap font (uppercase + digits + a few symbols) for baking labels
# --------------------------------------------------------------------------- #
_FONT = {
    "A": ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
    "B": ["11110", "10001", "11110", "10001", "10001", "10001", "11110"],
    "C": ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
    "D": ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
    "E": ["11111", "10000", "11110", "10000", "10000", "10000", "11111"],
    "F": ["11111", "10000", "11110", "10000", "10000", "10000", "10000"],
    "G": ["01110", "10001", "10000", "10111", "10001", "10001", "01111"],
    "H": ["10001", "10001", "11111", "10001", "10001", "10001", "10001"],
    "I": ["01110", "00100", "00100", "00100", "00100", "00100", "01110"],
    "J": ["00111", "00010", "00010", "00010", "10010", "10010", "01100"],
    "K": ["10001", "10010", "11100", "10100", "10010", "10001", "10001"],
    "L": ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
    "M": ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
    "N": ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
    "O": ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
    "P": ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
    "Q": ["01110", "10001", "10001", "10001", "10101", "10010", "01101"],
    "R": ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
    "S": ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
    "T": ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
    "U": ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
    "V": ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
    "W": ["10001", "10001", "10001", "10101", "10101", "11011", "10001"],
    "X": ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
    "Y": ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
    "Z": ["11111", "00001", "00010", "00100", "01000", "10000", "11111"],
    "0": ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
    "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
    "2": ["01110", "10001", "00001", "00110", "01000", "10000", "11111"],
    "3": ["11111", "00010", "00100", "00010", "00001", "10001", "01110"],
    "4": ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
    "5": ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
    "6": ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
    "7": ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
    "8": ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
    "9": ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
    " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
    "-": ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
    ".": ["00000", "00000", "00000", "00000", "00000", "00000", "00100"],
    ":": ["00000", "00100", "00000", "00000", "00000", "00100", "00000"],
}


def text_width(s, scale=1):
    return len(s) * 6 * scale


def draw_text(frame, x, y, s, color=(235, 235, 235), scale=2, shadow=True):
    """Draw `s` (auto-uppercased) at (x, y) with an optional 1px drop shadow."""
    s = s.upper()
    for ci, ch in enumerate(s):
        glyph = _FONT.get(ch, _FONT[" "])
        gx = x + ci * 6 * scale
        for ry, row in enumerate(glyph):
            for rx, bit in enumerate(row):
                if bit != "1":
                    continue
                for sy in range(scale):
                    for sx in range(scale):
                        px = gx + rx * scale + sx
                        py = y + ry * scale + sy
                        if shadow:
                            frame.put(px + scale, py + scale, (12, 12, 16))
                        frame.put(px, py, color)


def contact_sheet(frames, path, cols=3, shrink=2):
    """Tile `frames` (optionally box-downscaled by `shrink`) into a grid image."""
    if not frames:
        return
    thumbs = [fr.downscaled(shrink) if shrink > 1 else fr for fr in frames]
    tw, th = thumbs[0].w, thumbs[0].h
    rows = (len(thumbs) + cols - 1) // cols
    pad = 6
    sheet = Frame(cols * tw + (cols + 1) * pad, rows * th + (rows + 1) * pad, bg=(8, 9, 14))
    for k, fr in enumerate(thumbs):
        r, c = divmod(k, cols)
        sheet.blit(fr, pad + c * (tw + pad), pad + r * (th + pad))
    sheet.save(path)
