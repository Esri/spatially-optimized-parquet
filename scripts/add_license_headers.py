#!/usr/bin/env python3

import argparse
import os
from pathlib import Path
from typing import Iterable, Optional, Sequence


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
COMMENT_PREFIX_BY_SUFFIX = {
    ".rs": "//",
    ".ts": "//",
}
COMMENT_PREFIX_BY_FILE_NAME = {
    "Cargo.toml": "#",
}
EXCLUDED_DIRECTORY_NAMES = frozenset(
    {
        ".git",
        ".idea",
        ".next",
        ".vscode",
        "build",
        "coverage",
        "dist",
        "node_modules",
        "target",
    }
)
LICENSE_TEXT = """Copyright 2026 Esri

Licensed under the Apache License Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License."""


def main(arguments: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Add the Apache 2.0 license header to Rust, TypeScript, and Cargo manifest files."
        )
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Report files without the header instead of modifying them.",
    )
    parser.add_argument(
        "paths",
        nargs="*",
        type=Path,
        default=[REPOSITORY_ROOT],
        help="Files or directories to process. Defaults to the repository root.",
    )
    options = parser.parse_args(arguments)

    source_files = find_source_files(options.paths)
    missing_header_files = [
        source_file
        for source_file in source_files
        if not has_license_header(
            source_file.read_bytes(), resolve_comment_prefix(source_file)
        )
    ]

    if options.check:
        for source_file in missing_header_files:
            print(f"Missing license header: {display_path(source_file)}")
        return 1 if missing_header_files else 0

    for source_file in missing_header_files:
        source_file.write_bytes(
            add_license_header(
                source_file.read_bytes(), resolve_comment_prefix(source_file)
            )
        )

    print(f"Added license header to {len(missing_header_files)} file(s).")
    return 0


def find_source_files(paths: Iterable[Path]) -> list[Path]:
    source_files: set[Path] = set()

    for requested_path in paths:
        path = requested_path.resolve()
        if path.is_file():
            if resolve_comment_prefix(path):
                source_files.add(path)
            continue

        for directory_path, directory_names, file_names in os.walk(path):
            directory_names[:] = sorted(
                directory_name
                for directory_name in directory_names
                if directory_name not in EXCLUDED_DIRECTORY_NAMES
            )
            directory = Path(directory_path)
            source_files.update(
                directory / file_name
                for file_name in sorted(file_names)
                if resolve_comment_prefix(Path(file_name))
            )

    return sorted(source_files)


def has_license_header(source_bytes: bytes, comment_prefix: str) -> bool:
    source_text, _ = decode_source(source_bytes)
    header_offset = find_header_offset(source_text)
    normalized_source = source_text[header_offset:].replace("\r\n", "\n")
    return normalized_source.startswith(build_license_header(comment_prefix))


def add_license_header(source_bytes: bytes, comment_prefix: str) -> bytes:
    source_text, byte_order_mark = decode_source(source_bytes)
    if has_license_header(source_bytes, comment_prefix):
        return source_bytes

    newline = "\r\n" if "\r\n" in source_text else "\n"
    header = build_license_header(comment_prefix).replace("\n", newline)
    header_offset = find_header_offset(source_text)
    updated_source = (
        source_text[:header_offset]
        + header
        + newline
        + newline
        + source_text[header_offset:]
    )
    return byte_order_mark + updated_source.encode("utf-8")


def resolve_comment_prefix(path: Path) -> str:
    return COMMENT_PREFIX_BY_FILE_NAME.get(
        path.name, COMMENT_PREFIX_BY_SUFFIX.get(path.suffix, "")
    )


def build_license_header(comment_prefix: str) -> str:
    return "\n".join(
        f"{comment_prefix} {line}" if line else comment_prefix
        for line in LICENSE_TEXT.splitlines()
    )


def decode_source(source_bytes: bytes) -> tuple[str, bytes]:
    byte_order_mark = (
        b"\xef\xbb\xbf" if source_bytes.startswith(b"\xef\xbb\xbf") else b""
    )
    source_content = source_bytes[len(byte_order_mark) :]
    return source_content.decode("utf-8"), byte_order_mark


def find_header_offset(source_text: str) -> int:
    if not source_text.startswith("#!") or source_text.startswith("#!["):
        return 0

    first_line_end = source_text.find("\n")
    return len(source_text) if first_line_end == -1 else first_line_end + 1


def display_path(path: Path) -> Path:
    try:
        return path.relative_to(REPOSITORY_ROOT)
    except ValueError:
        return path


if __name__ == "__main__":
    raise SystemExit(main())
