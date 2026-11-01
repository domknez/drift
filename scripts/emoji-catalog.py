#!/usr/bin/env python3
"""Make assets/emoji/catalog.tsv from Teams' emoji catalog.

Usage: scripts/emoji-catalog.py <catalog url or file>

The catalog is the one the Teams web app loads, at
https://statics.teams.cdn.office.net/evergreen-assets/personal-expressions/v1/metadata/<hash>/en-us.json
(the hash changes with new emoji). Each line of the output is one emoji:
category, key, character, name, keywords (space-separated), popularity rank.
Categories come first, as lines starting with `#`.
"""

import json
import sys
import urllib.request

source = sys.argv[1]
if source.startswith("https://"):
    with urllib.request.urlopen(source) as answer:
        catalog = json.load(answer)
else:
    with open(source, encoding="utf-8") as f:
        catalog = json.load(f)


def clean(text):
    return " ".join(str(text).replace("\t", " ").split())


lines = []
for i, category in enumerate(catalog["categories"]):
    lines.append(f"#{i}\t{clean(category['title'])}")
for i, category in enumerate(catalog["categories"]):
    for e in category["emoticons"]:
        if not e.get("unicode"):
            continue
        keywords = " ".join(sorted({clean(k).lower() for k in e.get("keywords", []) if clean(k)}))
        rank = e.get("popularityRank", 9999)
        lines.append(f"{i}\t{e['id']}\t{e['unicode']}\t{clean(e['description'])}\t{keywords}\t{rank}")

with open("assets/emoji/catalog.tsv", "w", encoding="utf-8") as out:
    out.write("\n".join(lines) + "\n")
print(f"{len(lines)} lines")
