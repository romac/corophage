#!/usr/bin/env python3
"""Generate llms.txt and llms-full.txt from docs markdown files."""

import re
from pathlib import Path

DOCS_DIR = Path("content/docs")
PUBLIC_DIR = Path("public")
BASE_URL = "https://corophage.rs"

HEADER = """\
# corophage

> Algebraic effects for stable Rust

corophage lets you define side effects as plain structs, write business logic \
that yields those effects, and attach handlers separately. Programs are \
testable, composable, and run on stable Rust with no macros beyond the \
optional `#[effect]` and `#[effectful]` attributes.

- [Source Code](https://github.com/romac/corophage)
- [Crate](https://crates.io/crates/corophage)
- [API Documentation](https://docs.rs/corophage)
"""


def parse_page(path: Path):
    text = path.read_text()
    m = re.search(r'title\s*=\s*"(.+?)"', text)
    title = m.group(1) if m else path.stem
    m = re.search(r'weight\s*=\s*(\d+)', text)
    weight = int(m.group(1)) if m else 999
    m = re.search(r'description\s*=\s*"(.+?)"', text)
    desc = m.group(1) if m else ""
    slug = path.stem
    body = re.sub(r"\A\+\+\+.*?\+\+\+\s*", "", text, flags=re.DOTALL)
    return {"weight": weight, "title": title, "desc": desc, "slug": slug, "body": body}


def main():
    pages = []
    for f in DOCS_DIR.glob("*.md"):
        if f.name.startswith("_"):
            continue
        pages.append(parse_page(f))
    pages.sort(key=lambda p: p["weight"])

    PUBLIC_DIR.mkdir(parents=True, exist_ok=True)

    # llms.txt — index with links
    with open(PUBLIC_DIR / "llms.txt", "w") as out:
        out.write(HEADER)
        out.write("\n## Docs\n\n")
        for p in pages:
            url = f"{BASE_URL}/docs/{p['slug']}/"
            out.write(f"- [{p['title']}]({url}): {p['desc']}\n")

    # llms-full.txt — full inlined content
    with open(PUBLIC_DIR / "llms-full.txt", "w") as out:
        out.write(HEADER)
        for p in pages:
            out.write(f"\n---\n\n## {p['title']}\n\n")
            out.write(p["body"].strip())
            out.write("\n")


if __name__ == "__main__":
    main()
