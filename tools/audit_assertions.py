"""Find assertions in this workspace's tests that cannot fail.

Three shapes, all of which have shipped into this suite at least once:

  1. an assertion whose two operands are the same expression
     `assert_eq!(metadata(&p).modified().ok(), metadata(&p).modified().ok())`
  2. an assertion whose condition is a literal
     `assert!(true)`
  3. an assertion whose condition compares something to itself
     `assert!(a == a)`

A test built on one of these looks like coverage and is not. This walks every
`#[cfg(test)]` module in the crate, because the bug is invisible by reading a
name -- `asking_for_what_is_already_the_case_writes_nothing` sounded right.

WHAT THIS DOES NOT CATCH, which is most of what it is for
----------------------------------------------------------
The worst one found so far was not a self-comparison. It was

    assert!(!path.exists() || config::OverlayConfig::load_from(&path).is_some());

which is true no matter what happened: a readable file satisfies the second
half, an absent one satisfies the first. Deciding that needs real
satisfiability, not a scan. **A clean run of this tool is not evidence that the
tests are sound** -- it only rules out the three shapes above.

It also flags `assert_eq!(LITERAL, LITERAL)` written twice, because two identical
literals are indistinguishable from two calls to one function. Read those before
acting on them.

Run: python tools/audit_assertions.py <path>...
Expected output on a probe with one of each bad shape plus these three decoys
(`vec![(1,2)]` twice, a real assertion, a message containing a comma): 4
findings, one of them the `vec!` false positive.
"""

import re
import sys
from pathlib import Path

# `assert_eq!(a, b)` / `assert_ne!(a, b)`, tolerating nested parens by scanning
# for the comma that sits at depth zero.
COMPARE = re.compile(r"\bassert_(?:eq|ne)!\s*\(", re.MULTILINE)
TRUTHY = re.compile(r"\bassert!\s*\(", re.MULTILINE)


def split_top_level(text):
    """Split a macro call's arguments.

    `text` is everything *after* the opening `(`, so the walk starts at depth 1
    and depth returning to 0 is the closing paren of the macro itself. Splitting
    only at depth 1 keeps a comma inside `vec![(1, 2)]` where it belongs.

    A comma inside a *string* does count as a separator, which is harmless here:
    a format message is always the third argument or later, and the comparison
    that matters is between the first two.
    """
    args, depth, current = [], 1, []
    for char in text:
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                break
        if char == "," and depth == 1:
            args.append("".join(current).strip())
            current = []
            continue
        current.append(char)
    tail = "".join(current).strip()
    if tail:
        args.append(tail)
    return args, None


def operands_of(args):
    """The condition of an assertion: every argument before the first string
    literal, since anything from there on is a failure message."""
    condition = []
    for arg in args:
        if arg.lstrip().startswith('"'):
            break
        condition.append(arg)
    return ", ".join(condition)


def normalise(expr):
    """Collapse whitespace so `a  ==\n a` and `a == a` compare equal."""
    return re.sub(r"\s+", "", expr)


def audit(path):
    source = path.read_text(encoding="utf-8")
    if "#[cfg(test)]" not in source:
        return []

    findings = []
    lines = source.splitlines()
    for match in list(COMPARE.finditer(source)) + list(TRUTHY.finditer(source)):
        line_number = source[: match.start()].count("\n") + 1
        args, _ = split_top_level(source[match.end() :])
        condition = operands_of(args)
        if not condition:
            continue

        if normalise(condition) in {"true", "1", "!"}:
            findings.append((line_number, "condition is a literal", condition))
            continue

        # `assert_eq!(a, a)` -- the two operands are the same expression.
        if match.group(0).startswith("assert_") and len(args) >= 2:
            if normalise(args[0]) and normalise(args[0]) == normalise(args[1]):
                findings.append((line_number, "both sides are the same expression", args[0].strip()))
                continue

        # `assert!(a == a)` spelled with the operator instead.
        if "==" in condition:
            bits = re.split(r"==", condition, maxsplit=1)
            if len(bits) == 2 and normalise(bits[0]) == normalise(bits[1]):
                findings.append((line_number, "compares an expression to itself", condition))
    return findings


def sources(root):
    """Accept a directory to walk or a single file to check."""
    if root.is_file():
        return [root]
    return sorted(root.rglob("*.rs"))


def main():
    roots = [Path(a) for a in sys.argv[1:]] or [Path(".")]
    total = 0
    for root in roots:
        for path in sources(root):
            if "target" in path.parts:
                continue
            for line_number, why, snippet in audit(path):
                total += 1
                print(f"{path}:{line_number}: {why}\n    {snippet[:100]}")
    print(f"\nassertions that cannot fail: {total}")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())