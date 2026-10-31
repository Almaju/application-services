#!/usr/bin/env python3
"""Prototype repo checks for rules that are awkward to express as clippy or
ast-grep lints. Run from the repository root; prints a count per check and the
first few hits. Usage: checks.py [--all-hits]
"""

import collections
import re
import subprocess
import sys
from pathlib import Path

SHOW = 1000 if "--all-hits" in sys.argv else 5
ROOT = Path(".")


def tracked(*patterns):
    out = subprocess.run(["git", "ls-files", *patterns], capture_output=True, text=True, check=True).stdout
    return [Path(p) for p in out.splitlines() if Path(p).is_file()]


def is_test_path(p):
    s = str(p)
    return bool(re.search(r"(/tests?/|/testing/|_tests?\.rs$|/test_[^/]*\.rs$|/benches/|/examples/|/fixtures?/)", s))


def report(name, hits):
    print(f"\n== {name}: {len(hits)}")
    for h in hits[:SHOW]:
        print(f"   {h}")


# --------------------------------------------------------------------------
# UniFFI: exported names must be unique across one Swift module.

UDL_DECL = re.compile(
    r"^\s*(?:\[[^\]]*\]\s*)?(dictionary|interface|enum|callback interface|trait)\s+(\w+)", re.M)
UDL_FN = re.compile(r"^\s*(?:\[[^\]]*\]\s*)?[\w<>?]+\s+(\w+)\s*\(", re.M)
RS_DERIVE = re.compile(
    r"#\[derive\([^\]]*uniffi::(Record|Enum|Object|Error)[^\]]*\)\]\s*(?:#\[[^\]]*\]\s*|///[^\n]*\n\s*)*"
    r"pub(?:\([^)]*\))?\s+(?:struct|enum)\s+(\w+)", re.S)
RS_EXPORT_ITEM = re.compile(
    r"#\[uniffi::export(?:\([^)]*\))?\]\s*(?:#\[[^\]]*\]\s*)*pub\s+(?:trait|fn|async fn)\s+(\w+)")
RS_CUSTOM = re.compile(r"uniffi::custom_(?:type|newtype)!\s*\(\s*(\w+)")


def megazord_crates():
    lib = (ROOT / "megazords/ios-rust/src/lib.rs").read_text()
    names = re.findall(r"^pub use (\w+);", lib, re.M)
    meta = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"],
                          capture_output=True, text=True, check=True).stdout
    import json
    pkgs = {p["name"].replace("-", "_"): Path(p["manifest_path"]).parent for p in json.loads(meta)["packages"]}
    return {n: pkgs[n] for n in names if n in pkgs}


def exported_names(crate_dir):
    names = set()
    for udl in crate_dir.rglob("*.udl"):
        text = udl.read_text()
        names.update(m.group(2) for m in UDL_DECL.finditer(text))
        ns = re.search(r"namespace\s+\w+\s*\{(.*?)\n\};", text, re.S)
        if ns:
            names.update(UDL_FN.findall(ns.group(1)))
    for rs in crate_dir.rglob("*.rs"):
        if is_test_path(rs.relative_to(ROOT.resolve()) if rs.is_absolute() else rs):
            continue
        text = rs.read_text(errors="replace")
        if "uniffi" not in text:
            continue
        names.update(m.group(2) for m in RS_DERIVE.finditer(text))
        names.update(RS_EXPORT_ITEM.findall(text))
        names.update(RS_CUSTOM.findall(text))
    return names


def check_unique_exported_names():
    owners = collections.defaultdict(list)
    for crate, path in megazord_crates().items():
        for name in exported_names(path):
            owners[name].append(crate)
    hits = [f"duplicate {n}: {', '.join(sorted(c))}" for n, c in sorted(owners.items()) if len(c) > 1]
    hits += [f"collision-prone generic name {n} ({c[0]})" for n, c in sorted(owners.items())
             if n in GENERIC_NAMES and len(c) == 1]
    return hits


GENERIC_NAMES = {"ApiError", "Error", "Record", "Level", "Method", "Headers", "Request", "Response", "Backend",
                 "Attachment", "Config", "Metadata", "Client", "Store", "Settings", "Entry", "Item", "Status",
                 "Initialize", "initialize", "Callback", "Event", "Context"}


# --------------------------------------------------------------------------
# UDL declarations without a `///` docstring.

def check_udl_docs():
    hits = []
    decl = re.compile(r"^\s*(?:\[[^\]]*\]\s*)?(dictionary|interface|enum|callback interface)\s+\w+|"
                      r"^\s+(?:\[[^\]]*\]\s*)?(?:constructor|[\w<>?]+\s+\w+)\s*\(")
    for udl in tracked("*.udl"):
        lines = udl.read_text().splitlines()
        for i, line in enumerate(lines):
            if decl.match(line) and not line.strip().startswith("//"):
                prev = lines[i - 1].strip() if i else ""
                if not prev.startswith("///"):
                    hits.append(f"{udl}:{i + 1}: {line.strip()[:80]}")
    return hits


# --------------------------------------------------------------------------
# Proc-macro exported types/functions without a `///` docstring.

def check_proc_macro_docs():
    hits = []
    marker = re.compile(r"#\[(derive\([^\]]*uniffi::(Record|Enum|Object|Error)|uniffi::export)")
    for rs in tracked("components/**/*.rs"):
        if is_test_path(rs):
            continue
        lines = rs.read_text(errors="replace").splitlines()
        for i, line in enumerate(lines):
            if not marker.search(line):
                continue
            # The item is the next non-attribute line; docs may sit above or between attributes.
            j = i
            while j + 1 < len(lines) and lines[j + 1].strip().startswith(("#[", "///", "//")):
                j += 1
            item = lines[j + 1].strip() if j + 1 < len(lines) else ""
            if not re.match(r"(pub(\([^)]*\))?\s+)?(struct|enum|fn|async fn|trait)\b", item):
                continue  # `impl` blocks: methods are checked by missing_docs
            k = i
            while k - 1 >= 0 and lines[k - 1].strip().startswith("#["):
                k -= 1
            block = lines[max(k - 1, 0):j + 1]
            if not any(l.strip().startswith("///") for l in block):
                hits.append(f"{rs}:{j + 2}: {item[:80]}")
    return hits


# --------------------------------------------------------------------------
# Functions returning ApiResult without #[handle_error].

def check_apiresult_without_handle_error():
    hits = []
    for rs in tracked("components/**/*.rs"):
        if is_test_path(rs):
            continue
        lines = rs.read_text(errors="replace").splitlines()
        for i, line in enumerate(lines):
            if re.search(r"\bfn\s+\w+", line) and re.search(r"->\s*ApiResult<", " ".join(lines[i:i + 6]).split("{")[0]):
                k, attrs = i, []
                while k - 1 >= 0 and lines[k - 1].strip().startswith(("#[", "///", "//")):
                    k -= 1
                    attrs.append(lines[k])
                if not any("handle_error" in a for a in attrs):
                    hits.append(f"{rs}:{i + 1}: {line.strip()[:90]}")
    return hits


# --------------------------------------------------------------------------
# TODO/FIXME/XXX without a bug reference.

TODO = re.compile(r"\b(TODO|FIXME|XXX)\b")
BUG_REF = re.compile(r"(bug\s*#?\d{5,}|bugzil\.?la|bugzilla\.mozilla\.org|github\.com/\S+/(issues|pull)/\d+|#\d{3,}|"
                     r"\b[A-Z]{2,}-\d+\b|\d{6,})", re.I)


def check_todo_refs():
    hits = []
    for f in tracked("components/**/*.rs", "components/**/*.kt", "components/**/*.swift", "components/**/*.udl"):
        for i, line in enumerate(f.read_text(errors="replace").splitlines()):
            if TODO.search(line) and not BUG_REF.search(line) and re.search(r"(//|#|/\*|\*)", line):
                hits.append(f"{f}:{i + 1}: {line.strip()[:90]}")
    return hits


# --------------------------------------------------------------------------
# MPL header and final newline.

def check_license_header():
    hits = []
    for f in tracked("components/**/*.rs", "components/**/*.kt", "components/**/*.swift", "components/**/*.udl",
                     "megazords/**/*.rs", "tools/**/*.py", "automation/**/*.py", "automation/**/*.sh",
                     "taskcluster/**/*.py", "components/**/*.sh", "megazords/**/*.sh"):
        head = "\n".join(f.read_text(errors="replace").splitlines()[:12])
        if f.stat().st_size and "Mozilla Public" not in head and "MPL" not in head:
            hits.append(str(f))
    return hits


def check_final_newline():
    hits = []
    for f in tracked("*.rs", "*.kt", "*.swift", "*.udl", "*.py", "*.sh", "*.toml", "*.yaml", "*.yml", "*.md", "*.gradle"):
        data = f.read_bytes()
        if data and not data.endswith(b"\n"):
            hits.append(str(f))
    return hits


# --------------------------------------------------------------------------
# Commented-out code (rough heuristic).

COMMENTED_CODE = re.compile(
    r"^\s*//(?![/!])\s*(let\s+\w+|fn\s+\w+|pub\s+(fn|struct|enum)|use\s+\w+::|return\b|if\s+.*\{$|for\s+.*\{$|"
    r"[\w.:]+\(.*\);$|\w+\s*=\s*.+;$|\}$|assert(_eq|_ne)?!\()")


def check_commented_out_code():
    hits = []
    for f in tracked("components/**/*.rs"):
        for i, line in enumerate(f.read_text(errors="replace").splitlines()):
            if COMMENTED_CODE.match(line):
                hits.append(f"{f}:{i + 1}: {line.strip()[:90]}")
    return hits


# --------------------------------------------------------------------------
# SQL: foreign keys without ON DELETE, PRAGMAs outside initializers,
# migrations that reuse the live schema.

def check_fk_on_delete():
    hits = []
    for f in tracked("components/**/*.rs", "components/**/*.sql"):
        if is_test_path(f):
            continue
        text = f.read_text(errors="replace")
        for m in re.finditer(r"REFERENCES\s+\w+\s*\([^)]*\)([^,\n)]*)", text, re.I):
            if "ON DELETE" not in m.group(1).upper():
                line = text.count("\n", 0, m.start()) + 1
                hits.append(f"{f}:{line}: {m.group(0).strip()[:80]}")
    return hits


def check_pragma_outside_initializer():
    hits = []
    for f in tracked("components/**/*.rs"):
        if is_test_path(f):
            continue
        lines = f.read_text(errors="replace").splitlines()
        current_fn = ""
        for i, line in enumerate(lines):
            m = re.search(r"\bfn\s+(\w+)", line)
            if m:
                current_fn = m.group(1)
            if line.strip().startswith(("//", "--", "*")) or current_fn.startswith("test"):
                continue
            if re.search(r"\bPRAGMA\b", line) and not re.search(r"prepare|init|setup|open|upgrade|create|migrat", current_fn):
                hits.append(f"{f}:{i + 1}: in fn {current_fn}: {line.strip()[:70]}")
    return hits


LITERAL_VERSION = re.compile(
    r"(DB_VERSION\)\?,\s*Some\(\d+|migrated_version:\s*Some\(\d+\)|user_version.*(,|==)\s*\d+\s*\)|"
    r"(get_user_version|db_version|schema_version)\(\)\??\s*,\s*\d+\s*\))")


def check_migration_tests_literal_version():
    hits = []
    for f in tracked("components/**/*.rs"):
        for i, line in enumerate(f.read_text(errors="replace").splitlines()):
            if LITERAL_VERSION.search(line):
                hits.append(f"{f}:{i + 1}: {line.strip()[:90]}")
    return hits


# --------------------------------------------------------------------------
# report_error! labels: component-prefixed and unique.

def check_report_error_labels():
    labels = collections.defaultdict(list)
    for f in tracked("components/**/*.rs"):
        for i, line in enumerate(f.read_text(errors="replace").splitlines()):
            m = re.search(r"report_error(?:!\(|\()\s*\"([^\"]+)\"", line)
            if m:
                labels[m.group(1)].append(f"{f}:{i + 1}")
    hits = [f"duplicate label {l!r}: {', '.join(locs)}" for l, locs in labels.items() if len(locs) > 1]
    hits += [f"label not kebab-case {l!r}: {locs[0]}" for l, locs in labels.items()
             if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)+", l)]
    return hits


# --------------------------------------------------------------------------
# Kotlin `!!` and Swift force unwrap / try! / as! in hand-written wrappers.

def check_force_unwraps():
    hits = []
    for f in tracked("components/**/android/src/main/**/*.kt"):
        for i, line in enumerate(f.read_text(errors="replace").splitlines()):
            if "!!" in line and not line.strip().startswith("//"):
                hits.append(f"{f}:{i + 1}: {line.strip()[:80]}")
    for f in tracked("components/**/ios/**/*.swift", "megazords/ios-rust/Sources/**/*.swift"):
        if "/Generated/" in str(f) or "Tests" in str(f):
            continue
        for i, line in enumerate(f.read_text(errors="replace").splitlines()):
            if re.search(r"\btry!|\bas!|\w[\])]?!\.", line) and not line.strip().startswith("//"):
                hits.append(f"{f}:{i + 1}: {line.strip()[:80]}")
    return hits


# --------------------------------------------------------------------------
# Shell scripts without strict mode.

def check_shell_strict():
    hits = []
    for f in tracked("*.sh"):
        text = f.read_text(errors="replace")
        if not re.search(r"^\s*set\s+-[a-z]*e", text, re.M) and "set -o errexit" not in text:
            hits.append(str(f))
    return hits


CHECKS = [
    ("uniffi-unique-exported-names (iOS megazord)", check_unique_exported_names),
    ("uniffi-docstrings: UDL declarations without ///", check_udl_docs),
    ("uniffi-docstrings: proc-macro exported items without ///", check_proc_macro_docs),
    ("uniffi-handle-error: fn -> ApiResult without #[handle_error]", check_apiresult_without_handle_error),
    ("todo-needs-bug", check_todo_refs),
    ("license-header (MPL) missing", check_license_header),
    ("final newline missing", check_final_newline),
    ("no-commented-out-code (heuristic)", check_commented_out_code),
    ("schema-constraints: FOREIGN KEY without ON DELETE", check_fk_on_delete),
    ("pragmas-centralized: PRAGMA outside an init/upgrade fn", check_pragma_outside_initializer),
    ("migration-code-frozen: literal schema version in asserts", check_migration_tests_literal_version),
    ("error-report-labels", check_report_error_labels),
    ("no-force-unwrap-wrappers (Kotlin !!, Swift try!/as!/x!.)", check_force_unwraps),
    ("shell-strict: *.sh without set -e", check_shell_strict),
]

if __name__ == "__main__":
    for name, fn in CHECKS:
        report(name, fn())
