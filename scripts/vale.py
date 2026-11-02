"""Run Vale on Markdown files and on the comments in Rust files.

Vale reads each line of a Rust comment as its own snippet, so it lints the
code inside doctest fences. Each Rust file is linted as Markdown instead: the
comment text with its markers removed, and code lines blanked so the line
numbers still match.

Usage: vale.py FILE...
"""

import pathlib
import re
import subprocess
import sys
import tempfile

COMMENT = re.compile(r"^\s*//[/!]?\s?(.*)$")


def comment_text(source):
    return "".join(
        (match.group(1) if (match := COMMENT.match(line)) else "") + "\n"
        for line in source.splitlines()
    )


def unpublished():
    """Paths listed in .publicignore, if the file exists; they aren't linted."""
    ignore = pathlib.Path(".publicignore")
    if not ignore.is_file():
        return ()
    return tuple(
        line.strip().rstrip("/")
        for line in ignore.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    )


def main(paths):
    skip = unpublished()
    paths = [
        path
        for path in paths
        if not any(path == s or path.startswith(s + "/") for s in skip)
    ]
    config = pathlib.Path(".vale.ini").resolve()
    with tempfile.TemporaryDirectory() as work:
        targets = []
        for path in paths:
            if path.endswith(".rs"):
                target = pathlib.Path(work, path + ".md")
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(comment_text(pathlib.Path(path).read_text()))
                targets.append(str(target))
            else:
                targets.append(path)
        if not targets:
            return 0
        result = subprocess.run(
            ["vale", "--config", str(config), "--output", "line", *targets],
            capture_output=True,
            text=True,
            check=False,
        )
        prefix = work + "/"
        output = result.stdout.replace(prefix, "").replace(".rs.md:", ".rs:")
        sys.stdout.write(output)
        sys.stderr.write(result.stderr)
        return result.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
