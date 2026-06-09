#!/usr/bin/env python3
"""Shared low-poly concept render kit (no dependencies).

A tiny software renderer: compose objects from boxes / prisms / pyramids, then
flat-shade and paint them in the game's iso camera (yaw 45 deg, pitch ~0.95 rad).
Used by the building and unit/resource concept sheets. Design drafts only.
"""

import math
import struct
import zlib

SS = 2  # supersample factor (callers downsample at the end)

# ---------------------------------------------------------------- PNG output ---


def write_png(path, w, h, rgba):
    def chunk(typ, data):
        return (
            struct.pack(">I", len(data))
            + typ
            + data
            + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)
        )

    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw += rgba[y * w * 4 : (y + 1) * w * 4]
    out = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(out)


# ------------------------------------------------------------------ 3d math ---


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def norm(a):
    m = math.sqrt(dot(a, a)) or 1.0
    return (a[0] / m, a[1] / m, a[2] / m)


def newell(pts):
    n = [0.0, 0.0, 0.0]
    k = len(pts)
    for i in range(k):
        a = pts[i]
        b = pts[(i + 1) % k]
        n[0] += (a[1] - b[1]) * (a[2] + b[2])
        n[1] += (a[2] - b[2]) * (a[0] + b[0])
        n[2] += (a[0] - b[0]) * (a[1] + b[1])
    return norm((n[0], n[1], n[2]))


def centroid(pts):
    k = len(pts)
    return (sum(p[0] for p in pts) / k, sum(p[1] for p in pts) / k, sum(p[2] for p in pts) / k)


# Camera basis (matches camera.rs: yaw 45, pitch 0.95 rad).
YAW = math.pi / 4.0
PITCH = 0.95
_d = norm((math.cos(YAW) * math.cos(PITCH), math.sin(PITCH), math.sin(YAW) * math.cos(PITCH)))
FWD = _d  # scene -> camera
RIGHT = norm(cross((0, 1, 0), FWD))
CAMUP = cross(FWD, RIGHT)
LIGHT = norm((-0.45, 0.92, 0.32))
FILL = norm((0.5, 0.25, -0.6))  # weak back-fill so shadowed faces stay readable


def view(p):
    return (dot(p, RIGHT), dot(p, CAMUP), dot(p, FWD))


# ---------------------------------------------------------------- shape kit ---
# A mesh is a list of faces: (pts3d, (r,g,b), emissive_bool).


def box(mesh, p0, p1, color, emissive=False):
    x0, y0, z0 = p0
    x1, y1, z1 = p1
    v = [
        (x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1),
        (x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1),
    ]
    for q in [(4, 5, 6, 7), (0, 3, 2, 1), (3, 7, 6, 2), (0, 1, 5, 4), (1, 2, 6, 5), (0, 4, 7, 3)]:
        mesh.append(([v[i] for i in q], color, emissive))


def ring(n, cx, cz, r, y, rot=0.0):
    return [
        (cx + r * math.cos(rot + 2 * math.pi * k / n), y, cz + r * math.sin(rot + 2 * math.pi * k / n))
        for k in range(n)
    ]


def prism(mesh, cx, cz, r, y0, y1, color, n=8, rot=0.0, top=True, bottom=False, emissive=False):
    lo = ring(n, cx, cz, r, y0, rot)
    hi = ring(n, cx, cz, r, y1, rot)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([lo[k], lo[j], hi[j], hi[k]], color, emissive))
    if top:
        mesh.append((list(reversed(hi)), color, emissive))
    if bottom:
        mesh.append((lo, color, emissive))


def frustum(mesh, cx, cz, r0, r1, y0, y1, color, n=8, rot=0.0, top=True, emissive=False):
    lo = ring(n, cx, cz, r0, y0, rot)
    hi = ring(n, cx, cz, r1, y1, rot)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([lo[k], lo[j], hi[j], hi[k]], color, emissive))
    if top:
        mesh.append((list(reversed(hi)), color, emissive))


def pyramid(mesh, cx, cz, r, y0, y1, color, n=4, rot=math.pi / 4, emissive=False):
    base = ring(n, cx, cz, r, y0, rot)
    apex = (cx, y1, cz)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([base[k], base[j], apex], color, emissive))


def crystal(mesh, cx, cz, r, y0, ymid, ytip, color, tip_color, n=6, rot=0.0):
    prism(mesh, cx, cz, r, y0, ymid, color, n=n, rot=rot, top=False)
    base = ring(n, cx, cz, r, ymid, rot)
    apex = (cx, ytip, cz)
    for k in range(n):
        j = (k + 1) % n
        mesh.append(([base[k], base[j], apex], tip_color, False))


def gable(mesh, x0, z0, x1, z1, y0, yk, color):
    zm = (z0 + z1) / 2.0
    a = (x0, y0, z0); b = (x1, y0, z0); c = (x1, y0, z1); d = (x0, y0, z1)
    r0 = (x0, yk, zm); r1 = (x1, yk, zm)
    mesh.append(([a, b, r1, r0], color, False))
    mesh.append(([d, r0, r1, c], color, False))
    mesh.append(([a, r0, d], color, False))
    mesh.append(([b, c, r1], color, False))


# ------------------------------------------------------------------ raster ---


class Canvas:
    def __init__(self, w, h):
        self.w = w
        self.h = h
        self.buf = bytearray(w * h * 3)

    def set(self, x, y, rgb):
        if 0 <= x < self.w and 0 <= y < self.h:
            o = (y * self.w + x) * 3
            self.buf[o] = rgb[0]; self.buf[o + 1] = rgb[1]; self.buf[o + 2] = rgb[2]

    def blend(self, x, y, rgb, a):
        if 0 <= x < self.w and 0 <= y < self.h:
            o = (y * self.w + x) * 3
            self.buf[o] = int(self.buf[o] * (1 - a) + rgb[0] * a)
            self.buf[o + 1] = int(self.buf[o + 1] * (1 - a) + rgb[1] * a)
            self.buf[o + 2] = int(self.buf[o + 2] * (1 - a) + rgb[2] * a)

    def fill_rect(self, x0, y0, x1, y1, rgb):
        for y in range(max(0, y0), min(self.h, y1)):
            o = (y * self.w + max(0, x0)) * 3
            for x in range(max(0, x0), min(self.w, x1)):
                self.buf[o] = rgb[0]; self.buf[o + 1] = rgb[1]; self.buf[o + 2] = rgb[2]
                o += 3


def clampc(c):
    return (
        0 if c[0] < 0 else 255 if c[0] > 255 else int(c[0]),
        0 if c[1] < 0 else 255 if c[1] > 255 else int(c[1]),
        0 if c[2] < 0 else 255 if c[2] > 255 else int(c[2]),
    )


def fill_poly(cv, pts, rgb):
    if len(pts) < 3:
        return
    ys = [p[1] for p in pts]
    y0 = max(0, int(math.floor(min(ys))))
    y1 = min(cv.h - 1, int(math.ceil(max(ys))))
    n = len(pts)
    for y in range(y0, y1 + 1):
        yc = y + 0.5
        xs = []
        for i in range(n):
            ax, ay = pts[i]
            bx, by = pts[(i + 1) % n]
            if (ay <= yc < by) or (by <= yc < ay):
                t = (yc - ay) / (by - ay)
                xs.append(ax + t * (bx - ax))
        if len(xs) < 2:
            continue
        xs.sort()
        for k in range(0, len(xs) - 1, 2):
            xa = int(math.ceil(xs[k] - 0.5))
            xb = int(math.floor(xs[k + 1] - 0.5))
            o = (y * cv.w + max(0, xa)) * 3
            for x in range(max(0, xa), min(cv.w - 1, xb) + 1):
                cv.buf[o] = rgb[0]; cv.buf[o + 1] = rgb[1]; cv.buf[o + 2] = rgb[2]
                o += 3


def draw_line(cv, x0, y0, x1, y1, rgb):
    x0, y0, x1, y1 = int(x0), int(y0), int(x1), int(y1)
    dx = abs(x1 - x0); dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx + dy
    while True:
        cv.set(x0, y0, rgb)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy; x0 += sx
        if e2 <= dx:
            err += dx; y0 += sy


def shade(color, normal):
    diff = max(0.0, dot(normal, LIGHT))
    fill = max(0.0, dot(normal, FILL)) * 0.18
    inten = 0.40 + 0.72 * diff + fill
    return clampc((color[0] * inten, color[1] * inten, color[2] * inten))


def render_object(cv, mesh, cx, cy, cell_w, cell_h, fit=0.9, ground_r=4.6, shadow_r=4.6,
                  shadow_a=0.5):
    """Auto-fit a mesh into a cell and paint it (painter's algorithm + edges).

    `ground_r` frames in the ground footprint (so hovering objects show the gap
    to their shadow); `shadow_r` sizes the contact-shadow ellipse. Set them to 0
    to disable.
    """
    bc = centroid([p for f in mesh for p in f[0]])
    vmin = [1e9, 1e9]
    vmax = [-1e9, -1e9]
    fit_pts = [p for f in mesh for p in f[0]]
    if ground_r > 0:
        fit_pts += [(0, 0, 0), (ground_r, 0, ground_r), (-ground_r, 0, ground_r),
                    (ground_r, 0, -ground_r), (-ground_r, 0, -ground_r)]
    for p in fit_pts:
        vx, vy, _ = view(p)
        vmin[0] = min(vmin[0], vx); vmax[0] = max(vmax[0], vx)
        vmin[1] = min(vmin[1], vy); vmax[1] = max(vmax[1], vy)
    bw = (vmax[0] - vmin[0]) or 1.0
    bh = (vmax[1] - vmin[1]) or 1.0
    scale = min(cell_w * fit / bw, cell_h * fit / bh)
    mx = (vmin[0] + vmax[0]) / 2.0
    my = (vmin[1] + vmax[1]) / 2.0

    def proj(p):
        vx, vy, vz = view(p)
        return ((vx - mx) * scale + cx, cy - (vy - my) * scale, vz)

    if shadow_r > 0:
        gv = proj((0, 0, 0))
        rxs = scale * shadow_r
        rys = scale * shadow_r * 0.52
        for yy in range(int(gv[1] - rys), int(gv[1] + rys)):
            dy = (yy - gv[1]) / rys
            if abs(dy) > 1:
                continue
            half = rxs * math.sqrt(max(0.0, 1 - dy * dy))
            for xx in range(int(gv[0] - half), int(gv[0] + half)):
                cv.blend(xx, yy, (6, 9, 16), shadow_a)

    faces = []
    for pts, color, emis in mesh:
        nrm = newell(pts)
        fc = centroid(pts)
        if dot(nrm, sub(fc, bc)) < 0:
            nrm = (-nrm[0], -nrm[1], -nrm[2])
        if not emis and dot(nrm, FWD) <= 0.02:
            continue
        pp = [proj(p) for p in pts]
        depth = sum(p[2] for p in pp) / len(pp)
        col = color if emis else shade(color, nrm)
        faces.append((depth, [(p[0], p[1]) for p in pp], col, emis))
    faces.sort(key=lambda f: f[0])
    for _, p2, col, emis in faces:
        fill_poly(cv, p2, col)
        edge = (min(255, col[0] + 40), min(255, col[1] + 40), min(255, col[2] + 40)) if emis \
            else (int(col[0] * 0.55), int(col[1] * 0.55), int(col[2] * 0.55))
        for i in range(len(p2)):
            a = p2[i]; b = p2[(i + 1) % len(p2)]
            draw_line(cv, a[0], a[1], b[0], b[1], edge)


def downsample(cv):
    """Box-downsample the SS canvas to RGBA bytes at 1x."""
    fw, fh = cv.w // SS, cv.h // SS
    out = bytearray(fw * fh * 4)
    for y in range(fh):
        for x in range(fw):
            r = g = b = 0
            for sy in range(SS):
                for sx in range(SS):
                    o = ((y * SS + sy) * cv.w + (x * SS + sx)) * 3
                    r += cv.buf[o]; g += cv.buf[o + 1]; b += cv.buf[o + 2]
            n = SS * SS
            o = (y * fw + x) * 4
            out[o] = r // n; out[o + 1] = g // n; out[o + 2] = b // n; out[o + 3] = 255
    return fw, fh, out


# -------------------------------------------------------------------- font ---

FONT = {
    "A": ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
    "B": ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
    "C": ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
    "D": ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
    "E": ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
    "F": ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
    "G": ["01110", "10001", "10000", "10111", "10001", "10001", "01110"],
    "H": ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
    "I": ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
    "J": ["00111", "00010", "00010", "00010", "10010", "10010", "01100"],
    "K": ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
    "L": ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
    "M": ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
    "N": ["10001", "11001", "10101", "10101", "10011", "10001", "10001"],
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
    "-": ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
    "(": ["00100", "01000", "01000", "01000", "01000", "01000", "00100"],
    ")": ["00100", "00010", "00010", "00010", "00010", "00010", "00100"],
    "/": ["00001", "00001", "00010", "00100", "01000", "10000", "10000"],
    ".": ["00000", "00000", "00000", "00000", "00000", "00110", "00110"],
    ",": ["00000", "00000", "00000", "00000", "00110", "00110", "00100"],
    "+": ["00000", "00100", "00100", "11111", "00100", "00100", "00000"],
    " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
}


def text(cv, x, y, s, scale, rgb):
    cx = x
    for ch in s.upper():
        g = FONT.get(ch, FONT[" "])
        for ry in range(7):
            for rx in range(5):
                if g[ry][rx] == "1":
                    cv.fill_rect(cx + rx * scale, y + ry * scale,
                                 cx + rx * scale + scale, y + ry * scale + scale, rgb)
        cx += 6 * scale


def text_w(s, scale):
    return len(s) * 6 * scale


def gradient_bg(cv):
    for y in range(cv.h):
        t = y / cv.h
        cv.fill_rect(0, y, cv.w, y + 1,
                     (int(8 + 6 * (1 - t)), int(12 + 10 * (1 - t)), int(22 + 16 * (1 - t))))
