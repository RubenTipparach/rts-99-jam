#!/usr/bin/env python3
"""Preview sheet renderer for OBJ/MTL model assets (no dependencies).

Loads Wavefront models from assets/models/ and paints them with the concept
kit's iso camera so a model can be reviewed without booting the engine.
Unlike the concept sheets, faces here use the authored OBJ normals (the
models' normals point outward, so plain backface culling is correct).
Team-tint materials (`_t1`) render in a preview faction color. This is a
review tool: the in-engine look (detail maps, real lighting) comes from the
client's preview pipeline.

Run:  python3 assets/render_model_previews.py <name> [name ...]
      python3 assets/render_model_previews.py --sheet out.png <name ...>
Out:  assets/concepts/<out>.png (sheets) or assets/concepts/model-<name>.png
"""

import math
import os
import sys

from concept_kit import (
    SS, FWD, Canvas, view, dot, shade, fill_poly, draw_line, downsample,
    write_png, gradient_bg, text, text_w,
)

ASSETS = os.path.dirname(__file__)
MODELS = os.path.join(ASSETS, "models")
OUT_DIR = os.path.join(ASSETS, "concepts")
TEAM_PREVIEW = (130, 100, 230)  # how the _t1 team channel reads on the sheet


def load_mtl(path):
    mats = {}
    name = None
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if not parts:
                continue
            if parts[0] == "newmtl":
                name = parts[1]
            elif parts[0] == "Kd" and name:
                mats[name] = tuple(int(float(c) * 255) for c in parts[1:4])
    return mats


def load_obj(name):
    """Load an OBJ as a list of (pts, normal, color) faces."""
    path = os.path.join(MODELS, name + ".obj")
    verts = []
    normals = []
    faces = []
    mats = {}
    color = (200, 200, 200)
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if not parts:
                continue
            if parts[0] == "mtllib":
                mp = os.path.join(MODELS, parts[1])
                if os.path.exists(mp):
                    mats = load_mtl(mp)
            elif parts[0] == "v":
                verts.append(tuple(float(c) for c in parts[1:4]))
            elif parts[0] == "vn":
                normals.append(tuple(float(c) for c in parts[1:4]))
            elif parts[0] == "usemtl":
                mat = parts[1]
                color = TEAM_PREVIEW if "_t1" in mat else mats.get(mat, color)
            elif parts[0] == "f":
                pts = []
                nrm = (0.0, 1.0, 0.0)
                for p in parts[1:]:
                    idx = p.split("/")
                    pts.append(verts[int(idx[0]) - 1])
                    if len(idx) > 2 and idx[2]:
                        nrm = normals[int(idx[2]) - 1]
                faces.append((pts, nrm, color))
    return faces


def paint(cv, faces, cx, cy, cell, fit, shadow_r):
    """Project, depth-sort, and paint faces using their authored normals."""
    vmin = [1e9, 1e9]
    vmax = [-1e9, -1e9]
    pts_all = [p for f in faces for p in f[0]]
    pts_all += [(shadow_r, 0, shadow_r), (-shadow_r, 0, -shadow_r),
                (shadow_r, 0, -shadow_r), (-shadow_r, 0, shadow_r)]
    for p in pts_all:
        vx, vy, _ = view(p)
        vmin[0] = min(vmin[0], vx)
        vmax[0] = max(vmax[0], vx)
        vmin[1] = min(vmin[1], vy)
        vmax[1] = max(vmax[1], vy)
    scale = min(cell * fit / ((vmax[0] - vmin[0]) or 1.0),
                cell * fit / ((vmax[1] - vmin[1]) or 1.0))
    mx = (vmin[0] + vmax[0]) / 2.0
    my = (vmin[1] + vmax[1]) / 2.0

    def proj(p):
        vx, vy, vz = view(p)
        return ((vx - mx) * scale + cx, cy - (vy - my) * scale, vz)

    # Contact shadow at the ground origin.
    gv = proj((0, 0, 0))
    rxs = scale * shadow_r
    rys = rxs * 0.52
    for yy in range(int(gv[1] - rys), int(gv[1] + rys)):
        dy = (yy - gv[1]) / rys
        if abs(dy) > 1:
            continue
        half = rxs * math.sqrt(max(0.0, 1 - dy * dy))
        for xx in range(int(gv[0] - half), int(gv[0] + half)):
            cv.blend(xx, yy, (6, 9, 16), 0.45)

    drawn = []
    for pts, nrm, color in faces:
        if dot(nrm, FWD) <= 0.0:
            continue  # true backface: the authored normals point outward
        pp = [proj(p) for p in pts]
        depth = sum(p[2] for p in pp) / len(pp)
        drawn.append((depth, [(p[0], p[1]) for p in pp], shade(color, nrm)))
    drawn.sort(key=lambda f: f[0])
    for _, p2, col in drawn:
        fill_poly(cv, p2, col)
        edge = (int(col[0] * 0.6), int(col[1] * 0.6), int(col[2] * 0.6))
        for i in range(len(p2)):
            a = p2[i]
            b = p2[(i + 1) % len(p2)]
            draw_line(cv, a[0], a[1], b[0], b[1], edge)


def render_sheet(out_name, names, title):
    cols = 4
    rows = (len(names) + cols - 1) // cols
    cell = 260 * SS
    gap = 16 * SS
    margin = 30 * SS
    label_h = 26 * SS
    title_h = 50 * SS
    W = margin * 2 + cols * cell + (cols - 1) * gap
    H = margin + title_h + rows * (cell + label_h) + margin // 2
    cv = Canvas(W, H)
    gradient_bg(cv)
    text(cv, margin, margin, title, 3 * SS, (228, 234, 246))
    for i, name in enumerate(names):
        r, c = divmod(i, cols)
        x = margin + c * (cell + gap)
        y = margin + title_h + r * (cell + label_h)
        cv.fill_rect(x, y, x + cell, y + cell, (13, 18, 30))
        faces = load_obj(name)
        wide = max(max(abs(p[0]), abs(p[2])) for f in faces for p in f[0])
        paint(cv, faces, x + cell // 2, y + cell // 2 + 14 * SS, cell,
              0.74, wide * 0.7 + 0.5)
        lw = text_w(name.upper(), 2 * SS)
        text(cv, x + (cell - lw) // 2, y + cell + 5 * SS, name.upper(), 2 * SS,
             (208, 216, 230))
    fw, fh, out = downsample(cv)
    path = os.path.join(OUT_DIR, out_name)
    write_png(path, fw, fh, out)
    print("wrote", path, fw, "x", fh)


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return
    os.makedirs(OUT_DIR, exist_ok=True)
    if args[0] == "--sheet":
        render_sheet(args[1], args[2:], "MODEL PREVIEWS  -  " + args[1].upper())
    else:
        for name in args:
            render_sheet("model-" + name + ".png", [name], "MODEL PREVIEW")


if __name__ == "__main__":
    main()
