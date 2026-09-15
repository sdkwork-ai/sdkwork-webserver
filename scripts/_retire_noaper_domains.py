"""Retire `noaper.com` / `noaper.cn` hosts from the Web Server configuration.

`APP_RUNTIME_TOPOLOGY_NAMING.md` §9.3 (Base Domain Registry) enumerates the 14
registered base domains and does **not** include `noaper.com` / `noaper.cn`.
Every SDKWork-managed product must bind hosts on exactly that registry, so the
`*.noaper.*` hosts still present in this repository's hand-maintained
configuration are stale and must be retired.

Shapes handled, each by structure rather than by blind text replacement:

* `deployments/deploy.yaml` — whole YAML list items (`- tls: noaper.com` plus
  the block it owns).
* `deployments/webserver/nginx.*.conf` — whole `server { … }` blocks whose body
  binds a retired host.
* `deployments/webserver/server.*.toml` — whole `[[http.server]]` sections
  (including their `[http.server.tls]` sub-table).
* `specs/topology.spec.json` — single array elements that are a retired host.
* `.env` / docker-compose / `entrypoint-standalone.sh` host lists — only the
  retired comma- or space-separated members of a line that mentions them.
* `etc/sdkwork.deployment.config.json` — `;` separated URL lists, by text
  substitution so the document's formatting is preserved.

Files that legitimately *name* the retired domains (negative tests, this
script) are listed in `ALLOWED` and skipped. Run with `--check` to report
without writing.
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RETIRED = re.compile(r"(^|\.)noaper\.(com|cn)$")
YAML_ITEM = re.compile(r"^(\s*)-\s+\S")
QUOTED_HOST = re.compile(r'^\s*"[^"]*"\s*,?\s*$')

# Files where the retired domains appear on purpose.
ALLOWED = {
    "crates/sdkwork-api-webserver-standalone-gateway/src/deploy_fallback.rs",
    "crates/sdkwork-webserver-core/tests/webserver_config.rs",
    "scripts/_retire_noaper_domains.py",
}

SKIP_DIRECTORIES = {"target", ".git", "node_modules", ".workbuddy", "docs", "sdks"}
CONFIG_SUFFIXES = {".env", ".json", ".yaml", ".yml", ".conf", ".toml", ".sh", ".example"}


def is_retired(value: str) -> bool:
    host = value.strip().strip('"').strip("'")
    if not host:
        return False
    if "://" in host:
        host = host.split("://", 1)[1]
    host = host.split("/", 1)[0].split(":", 1)[0].split(";", 1)[0]
    return bool(RETIRED.search(host.lower()))


def mentions_retired(text: str) -> bool:
    return "noaper" in text.lower()


def strip_yaml_items(text: str) -> tuple[str, int]:
    """Drop whole YAML list items whose body names a retired host.

    The item key order is not fixed (`- tls: noaper.com` in the role-host
    blocks, `- domain: server-admin.noaper.com` in the API blocks), so the
    item is recognised by its `- ` marker and then judged by its whole body
    rather than by one key.
    """
    lines = text.splitlines(keepends=True)
    kept: list[str] = []
    removed = 0
    index = 0
    while index < len(lines):
        line = lines[index]
        match = YAML_ITEM.match(line.rstrip("\r\n"))
        if not match:
            kept.append(line)
            index += 1
            continue
        indent = len(match.group(1))
        start = index
        index += 1
        while index < len(lines):
            candidate = lines[index]
            if not candidate.strip():
                index += 1
                continue
            if len(candidate) - len(candidate.lstrip()) <= indent:
                break
            index += 1
        block = "".join(lines[start:index])
        if mentions_retired(block):
            removed += 1
        else:
            kept.append(block)
    return "".join(kept), removed


def strip_braced_blocks(text: str, opener: str) -> tuple[str, int]:
    """Drop every `<opener> { … }` block whose body mentions a retired host."""
    lines = text.splitlines(keepends=True)
    kept: list[str] = []
    removed = 0
    index = 0
    while index < len(lines):
        if not re.fullmatch(rf"{re.escape(opener)}\s*\{{", lines[index].strip()):
            kept.append(lines[index])
            index += 1
            continue
        start = index
        index += 1
        while index < len(lines) and lines[index].strip() != "}":
            index += 1
        end = min(index + 1, len(lines))
        block = "".join(lines[start:end])
        if mentions_retired(block):
            removed += 1
        else:
            kept.extend(lines[start:end])
        index = end
    return "".join(kept), removed


def strip_toml_sections(text: str) -> tuple[str, int]:
    """Drop whole TOML sections that mention a retired host."""
    preamble: list[str] = []
    sections: list[list[str]] = []
    current: list[str] | None = None
    for line in text.splitlines(keepends=True):
        if line.lstrip().startswith("["):
            current = [line]
            sections.append(current)
        elif current is None:
            preamble.append(line)
        else:
            current.append(line)
    kept: list[str] = list(preamble)
    removed = 0
    for section in sections:
        block = "".join(section)
        if mentions_retired(block):
            removed += 1
            continue
        kept.append(block)
    return "".join(kept), removed


def strip_quoted_host_lines(text: str) -> tuple[str, int]:
    """Drop JSON array elements that are exactly a retired host."""
    removed = 0
    kept: list[str] = []
    for line in text.splitlines(keepends=True):
        candidate = line.strip().rstrip(",").strip()
        if QUOTED_HOST.match(line) and is_retired(candidate):
            removed += 1
            continue
        kept.append(line)
    return "".join(kept), removed


def strip_host_list_lines(text: str) -> tuple[str, int]:
    """Drop retired members from comma lists, or from a space separated
    `base_domains` style shell variable. Only lines that mention the retired
    domains are rewritten."""
    removed = 0
    kept: list[str] = []
    for line in text.splitlines(keepends=True):
        if not mentions_retired(line):
            kept.append(line)
            continue
        body, newline, _ = line.partition("\n")
        if re.search(r'base_domains="', body):
            head, _, tail = body.partition('="')
            pieces = tail.rstrip('"').split(" ")
            survivors = [piece for piece in pieces if not is_retired(piece)]
            removed += len(pieces) - len(survivors)
            kept.append(f'{head}="{" ".join(survivors)}"{newline}')
            continue
        members = body.split(",")
        survivors = [member for member in members if not is_retired(member)]
        removed += len(members) - len(survivors)
        kept.append(",".join(survivors) + newline)
    return "".join(kept), removed


def strip_json_url_lists(text: str) -> tuple[str, int]:
    pattern = re.compile(r'https?://[^;"]*noaper\.(?:com|cn);?')
    removed = len(pattern.findall(text))
    updated = pattern.sub("", text)
    while ";;" in updated:
        updated = updated.replace(";;", ";")
    while ';"' in updated:
        updated = updated.replace(';"', '"')
    # The document must still be valid JSON after the substitution.
    json.loads(updated)
    return updated, removed


def strip_json_document(text: str) -> tuple[str, int]:
    """A JSON fixture can hold both array elements and `;` separated lists."""
    removed = 0
    for handler in (strip_quoted_host_lines, strip_json_url_lists):
        text, count = handler(text)
        removed += count
    json.loads(text)
    return text, removed


def handler_for(relative: Path):
    name = relative.as_posix()
    if name.endswith("deploy.yaml"):
        return strip_yaml_items
    if name.endswith(".conf"):
        return lambda text: strip_braced_blocks(text, "server")
    if name.endswith(".toml"):
        return strip_toml_sections
    if name.endswith(".json"):
        return strip_json_document
    return strip_host_list_lines


def config_paths():
    for directory, subdirectories, filenames in os.walk(ROOT):
        subdirectories[:] = [d for d in subdirectories if d not in SKIP_DIRECTORIES]
        for filename in sorted(filenames):
            path = Path(directory) / filename
            relative = path.relative_to(ROOT)
            if relative.as_posix() in ALLOWED:
                continue
            if path.suffix in CONFIG_SUFFIXES:
                yield path, relative


def parse_roots() -> list[Path]:
    roots: list[Path] = []
    arguments = sys.argv[1:]
    index = 0
    while index < len(arguments):
        if arguments[index] == "--root":
            roots.append(Path(arguments[index + 1]).resolve())
            index += 2
            continue
        index += 1
    return roots or [ROOT]


def main() -> int:
    check = "--check" in sys.argv
    global ROOT
    total = 0
    for root in parse_roots():
        ROOT = root
        print(f"== {root}")
        total += retire(root, check)
    print(f"total removed: {total}")
    for root in parse_roots():
        print(f"remaining references in {root}: " + (", ".join(residue(root)) or "none"))
    return 0


def retire(root: Path, check: bool) -> int:
    total = 0
    for path, relative in config_paths():
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if not mentions_retired(text):
            continue
        updated, removed = handler_for(relative)(text)
        total += removed
        print(f"{relative}: {removed}")
        if removed and not check:
            path.write_text(updated, encoding="utf-8", newline="")

    return total


def residue(root: Path) -> list[str]:
    global ROOT
    ROOT = root
    found: list[str] = []
    for path, relative in config_paths():
        try:
            if mentions_retired(path.read_text(encoding="utf-8", errors="ignore")):
                found.append(str(relative))
        except OSError:
            continue
    return sorted(found)


if __name__ == "__main__":
    raise SystemExit(main())
