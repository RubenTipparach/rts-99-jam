#!/usr/bin/env python3
"""Fetch a representative NASA reference image (metadata) per world.

Queries the public NASA Image and Video Library API
(https://images.nasa.gov, no key required) and records, for each world, a real
image's title, NASA id, capturing center/mission, date and thumbnail URL. The
result is written to `docs/worldgen/nasa_references.json` and cited by the
catalog (`worlds.py`) and `docs/worldgen.md`.

This is the "utilize reference images from NASA" step: the palettes in worlds.py
are tuned to match this imagery. Run occasionally to refresh; the committed JSON
is what the docs cite, so the pipeline does not need network at render time.

Run:  python3 assets/worldgen/fetch_nasa.py
"""

import json
import os
import time
import urllib.parse
import urllib.request

import worlds as cat

API = "https://images-api.nasa.gov/search"
OUT = os.path.join(os.path.dirname(__file__), "..", "..", "docs", "worldgen", "nasa_references.json")

# Tailored queries so we land on the right body / mission, not a namesake.
QUERIES = {
    "moon": "moon surface lunar reconnaissance orbiter",
    "ceres": "ceres occator dawn",
    "vesta": "vesta dawn asteroid",
    "mars": "mars surface viking",
    "callisto": "callisto galileo jupiter moon",
    "ganymede": "ganymede galileo jupiter moon",
    "europa": "europa galileo jupiter moon",
    "io": "io jupiter volcano galileo",
    "titan": "titan saturn cassini surface",
    "enceladus": "enceladus saturn tiger stripes cassini",
    "triton": "triton neptune voyager",
    "rhea": "rhea saturn cassini",
    "iapetus": "iapetus saturn cassini",
    "dione": "dione saturn cassini",
    "titania": "titania uranus voyager",
    "oberon": "oberon uranus voyager",
    "umbriel": "umbriel uranus voyager",
    "ariel": "ariel uranus voyager",
    "miranda": "miranda uranus voyager",
    "pluto": "pluto new horizons",
    # Chiron itself has no resolved NASA surface image (it is a distant centaur);
    # a comet nucleus is the closest visual analog for its icy-rock, outgassing look.
    "chiron": "comet nucleus",
}


def fetch_json(url, retries=3):
    last = None
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "astromancers-worldgen/1.0"})
            with urllib.request.urlopen(req, timeout=20) as r:
                return json.loads(r.read().decode("utf-8"))
        except Exception as e:  # noqa: BLE001 - best-effort tooling
            last = e
            time.sleep(2 * (attempt + 1))
    print(f"  ! request failed: {last}")
    return None


def first_image(query):
    url = API + "?" + urllib.parse.urlencode({"q": query, "media_type": "image"})
    data = fetch_json(url)
    if not data:
        return None
    items = data.get("collection", {}).get("items", [])
    for it in items:
        meta = (it.get("data") or [{}])[0]
        links = it.get("links") or []
        thumb = links[0].get("href") if links else None
        if not meta.get("nasa_id"):
            continue
        return {
            "title": meta.get("title", "").strip(),
            "nasa_id": meta.get("nasa_id"),
            "center": meta.get("center"),
            "date_created": meta.get("date_created"),
            "thumbnail": thumb,
            "description": (meta.get("description") or "").strip()[:280],
            "asset_page": "https://images.nasa.gov/details/" + meta.get("nasa_id", ""),
        }
    return None


def main():
    out = {
        "_about": "Representative NASA reference imagery per world, used to tune "
                  "the texture palettes in assets/worldgen/worlds.py. Imagery "
                  "credit: NASA/JPL-Caltech and partner institutions; most NASA "
                  "imagery is public domain (see https://www.nasa.gov/multimedia/guidelines/).",
        "source": "NASA Image and Video Library (images.nasa.gov)",
        "worlds": {},
    }
    for w in cat.WORLDS:
        key = w["key"]
        q = QUERIES.get(key, key)
        print(f"  {key:10s} <- {q}")
        ref = first_image(q)
        out["worlds"][key] = {
            "name": w["name"],
            "mission_hint": w["nasa"],
            "query": q,
            "reference": ref,
        }
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(out, f, indent=2)
        f.write("\n")
    found = sum(1 for v in out["worlds"].values() if v["reference"])
    print(f"wrote {os.path.relpath(OUT)} ({found}/{len(out['worlds'])} with a reference image)")


if __name__ == "__main__":
    main()
