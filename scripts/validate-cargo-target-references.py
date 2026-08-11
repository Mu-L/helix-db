#!/usr/bin/env python3
"""Validate Cargo package and target references in repository automation."""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable


REFERENCE = re.compile(
    r"(?<![A-Za-z0-9_-])(?P<flag>--package|--test|--bench|--bin|-p)"
    r"(?:=|\s+)(?P<name>[A-Za-z0-9_.-]+)"
)
INVENTORY_ROW = re.compile(
    r"^\| `(?P<name>[^`]+)` \| `(?P<kind>[^`]+)` \| `(?P<source>[^`]+)` \|"
)
MATRIX_FUZZ = re.compile(
    r"-\s+fuzz_dir:\s*(?P<directory>[^\s]+)\s+"
    r"target:\s*(?P<target>[A-Za-z0-9_.-]+)",
    re.MULTILINE,
)
MATRIX_TEST = re.compile(r"^\s+test:\s*(?P<test>[A-Za-z0-9_:.-]+)\s*$", re.MULTILINE)
FLAG_KIND = {"--test": "test", "--bench": "bench", "--bin": "bin"}
TARGET_KIND_ORDER = ("lib", "test", "bench", "bin")
CODEC_OWNERS = {
    "keys/global.rs": "enum GlobalKey",
    "keys/lifecycle.rs": "struct IndexRecordKey",
    "keys/indexes/equality.rs": "struct SecondaryEquality",
    "keys/indexes/range.rs": "struct SecondaryRange",
    "keys/indexes/text.rs": "struct TextManifestRootKey",
    "keys/indexes/vector.rs": "struct VectorPartitionMappingKey",
    "values/global.rs": "fn encode_metadata_value",
    "values/lifecycle.rs": "fn encode_index_record",
    "values/indexes/equality.rs": "struct SecondaryEqualityBitmapValue",
    "values/indexes/range.rs": "fn encode_entry",
    "values/indexes/text.rs": "fn encode_manifest_root",
    "values/indexes/vector.rs": "fn encode_partition_mapping",
}
RAW_RUNTIME_CODEC_EXEMPTIONS = {
    # Canonical hash preimages are model data, not stored database values.
    "crates/db/src/index_lifecycle/work.rs",
    # V3 tenant framing remains private to the blocking V3-to-V4 migration.
    "crates/db/src/index_lifecycle/tenant_envelope_migration.rs",
}


@dataclass(frozen=True)
class Target:
    package: str
    name: str
    kind: str
    source: str


@dataclass
class Catalog:
    packages: set[str] = field(default_factory=set)
    targets: dict[str, set[str]] = field(
        default_factory=lambda: {kind: set() for kind in TARGET_KIND_ORDER}
    )
    target_owners: dict[tuple[str, str], set[str]] = field(default_factory=dict)
    required_features: dict[tuple[str, str, str], tuple[str, ...]] = field(
        default_factory=dict
    )
    db_targets: set[Target] = field(default_factory=set)

    def add_metadata(self, metadata: dict[str, object], root: Path) -> None:
        for package in metadata["packages"]:
            package_name = package["name"]
            self.packages.add(package_name)
            for raw_target in package["targets"]:
                kinds = set(raw_target["kind"])
                kind = next((candidate for candidate in TARGET_KIND_ORDER if candidate in kinds), None)
                if kind is None:
                    continue
                name = raw_target["name"]
                self.targets[kind].add(name)
                self.target_owners.setdefault((kind, name), set()).add(package_name)
                self.required_features[(package_name, kind, name)] = tuple(
                    raw_target.get("required-features", ())
                )
                if package_name == "db":
                    source_path = Path(raw_target["src_path"])
                    try:
                        source = source_path.relative_to(root).as_posix()
                    except ValueError:
                        source = source_path.as_posix()
                    self.db_targets.add(Target(package_name, name, kind, source))


@dataclass(frozen=True)
class TestFilter:
    source: str
    package: str
    target: str
    features: tuple[str, ...]
    name: str


def run(command: list[str], cwd: Path) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"{' '.join(command)} failed:\n{completed.stdout}{completed.stderr}"
        )
    return completed.stdout


def workspace_manifests(root: Path) -> list[Path]:
    relative_paths = (
        "Cargo.toml",
        "bindings/uniffi-bindgen/Cargo.toml",
        "crates/db-testkit/fuzz/Cargo.toml",
        "crates/db/fuzz/Cargo.toml",
    )
    return [root / relative_path for relative_path in relative_paths]


def load_catalog(root: Path) -> Catalog:
    catalog = Catalog()
    for manifest in workspace_manifests(root):
        raw = run(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
                str(manifest),
            ],
            root,
        )
        catalog.add_metadata(json.loads(raw), root)
    return catalog


def yaml_run_commands(text: str) -> list[str]:
    lines = text.splitlines()
    commands = []
    line_index = 0
    while line_index < len(lines):
        match = re.match(
            r"^(?P<indent>\s*)(?:-\s+)?run:\s*(?P<body>.*)$", lines[line_index]
        )
        if match is None:
            line_index += 1
            continue
        body = match.group("body").strip()
        if body not in {"|", "|-", ">", ">-"}:
            commands.append(body)
            line_index += 1
            continue
        indentation = len(match.group("indent"))
        block = []
        line_index += 1
        while line_index < len(lines):
            line = lines[line_index]
            if line.strip() and len(line) - len(line.lstrip()) <= indentation:
                break
            block.append(line.strip())
            line_index += 1
        if body in {"|", "|-"}:
            commands.extend(shell_commands("\n".join(block)))
        else:
            commands.append(" ".join(block))
    return commands


def shell_commands(text: str) -> list[str]:
    joined = re.sub(r"\\\s*\n\s*", " ", text)
    return [line.strip() for line in joined.splitlines() if "cargo " in line]


def automation_files(root: Path) -> list[Path]:
    workflows = list((root / ".github" / "workflows").glob("*.yml"))
    workflows.extend((root / ".github" / "workflows").glob("*.yaml"))
    scripts = [
        path
        for path in (root / "scripts").iterdir()
        if path.suffix in {".sh", ".py"}
        and path.name != "validate-cargo-target-references.py"
    ]
    return sorted(workflows + scripts)


def commands(path: Path) -> list[str]:
    text = path.read_text()
    if path.suffix in {".yml", ".yaml"}:
        return yaml_run_commands(text)
    return shell_commands(text)


def validate_references(
    root: Path, catalog: Catalog, files: Iterable[Path]
) -> list[str]:
    errors = []
    for path in files:
        for command in commands(path):
            if "cargo " not in command:
                continue
            for match in REFERENCE.finditer(command):
                flag = match.group("flag")
                name = match.group("name")
                if flag in {"-p", "--package"}:
                    if name not in catalog.packages:
                        errors.append(f"{path.relative_to(root)}: unknown Cargo package {name!r}")
                    continue
                kind = FLAG_KIND[flag]
                if name not in catalog.targets[kind]:
                    errors.append(
                        f"{path.relative_to(root)}: unknown Cargo {kind} target {name!r}"
                    )
    return errors


def validate_fuzz_matrices(root: Path, catalog: Catalog) -> list[str]:
    errors = []
    for path in (root / ".github" / "workflows").glob("*.yml"):
        text = path.read_text()
        if "cargo fuzz run ${{ matrix.target }}" not in text:
            continue
        for match in MATRIX_FUZZ.finditer(text):
            directory = match.group("directory")
            target = match.group("target")
            manifest = root / directory / "Cargo.toml"
            if not manifest.is_file():
                errors.append(
                    f"{path.relative_to(root)}: fuzz workspace {directory!r} has no Cargo.toml"
                )
            if target not in catalog.targets["bin"]:
                errors.append(
                    f"{path.relative_to(root)}: unknown Cargo fuzz target {target!r}"
                )
    return errors


def parse_inventory(text: str) -> set[tuple[str, str, str]]:
    rows = set()
    for line in text.splitlines():
        match = INVENTORY_ROW.match(line)
        if match is not None:
            rows.add((match.group("name"), match.group("kind"), match.group("source")))
    return rows


def validate_inventory(root: Path, catalog: Catalog) -> list[str]:
    path = root / "crates" / "db" / "tests" / "TEST_TARGET_INVENTORY.md"
    documented = parse_inventory(path.read_text())
    actual = {(target.name, target.kind, target.source) for target in catalog.db_targets}
    errors = []
    for missing in sorted(actual - documented):
        errors.append(f"{path.relative_to(root)}: missing Cargo target {missing!r}")
    for stale in sorted(documented - actual):
        errors.append(f"{path.relative_to(root)}: stale Cargo target {stale!r}")
    return errors


def command_features(command: str) -> tuple[str, ...]:
    match = re.search(r"--features(?:=|\s+)(?P<features>[A-Za-z0-9_,.-]+)", command)
    if match is None:
        return ()
    return tuple(filter(None, match.group("features").split(",")))


def command_package(command: str, catalog: Catalog, target: str) -> str | None:
    matches = list(re.finditer(r"(?:-p|--package)(?:=|\s+)([A-Za-z0-9_.-]+)", command))
    if matches:
        return matches[-1].group(1)
    owners = catalog.target_owners.get(("test", target), set())
    return next(iter(owners)) if len(owners) == 1 else None


def referenced_test_filters(root: Path, catalog: Catalog) -> list[TestFilter]:
    filters = set()
    for path in automation_files(root):
        text = path.read_text()
        for command in commands(path):
            target_match = re.search(
                r"--test(?:=|\s+)(?P<target>[A-Za-z0-9_.-]+)", command
            )
            if target_match is None or "cargo test" not in command:
                continue
            target = target_match.group("target")
            package = command_package(command, catalog, target)
            if package is None:
                continue
            features = command_features(command) or catalog.required_features.get(
                (package, "test", target), ()
            )
            tail = command[target_match.end() :]
            filter_match = re.match(r"\s+(?!-)(?P<filter>[A-Za-z0-9_:.-]+)", tail)
            if filter_match is not None:
                filters.add(
                    TestFilter(
                        path.relative_to(root).as_posix(),
                        package,
                        target,
                        features,
                        filter_match.group("filter"),
                    )
                )
            if "${{ matrix.test }}" in tail:
                for name in MATRIX_TEST.findall(text):
                    filters.add(
                        TestFilter(
                            path.relative_to(root).as_posix(),
                            package,
                            target,
                            features,
                            name,
                        )
                    )
    return sorted(
        filters,
        key=lambda item: (item.package, item.target, item.features, item.name, item.source),
    )


def listed_tests(root: Path, group: tuple[str, str, tuple[str, ...]]) -> set[str]:
    package, target, features = group
    command = ["cargo", "test", "-p", package]
    if features:
        command.extend(["--features", ",".join(features)])
    command.extend(["--test", target, "--", "--list"])
    output = run(command, root)
    return {
        line.rsplit(": ", 1)[0]
        for line in output.splitlines()
        if line.endswith(": test")
    }


def validate_test_filters(root: Path, catalog: Catalog) -> list[str]:
    references = referenced_test_filters(root, catalog)
    listings = {}
    errors = []
    for reference in references:
        group = (reference.package, reference.target, reference.features)
        if group not in listings:
            listings[group] = listed_tests(root, group)
        if reference.name not in listings[group]:
            errors.append(
                f"{reference.source}: unknown exact test filter {reference.name!r} "
                f"for {reference.package}:{reference.target}"
            )
    return errors


def forbidden_codec_source(path: str, text: str) -> list[str]:
    errors = []
    for identifier in ("WorkValue", "decode_work_value"):
        if re.search(rf"\b{identifier}\b", text):
            errors.append(f"{path}: forbidden obsolete identifier {identifier!r}")
    if re.search(r"encoding::v1::(?:keys|values)::index_v2\b", text):
        errors.append(f"{path}: forbidden V1 index_v2 codec import")
    if re.search(r"\bmod\s+index_v2\s*;", text):
        errors.append(f"{path}: forbidden index_v2 runtime module")
    if (
        path.startswith("crates/db/src/index_lifecycle/")
        and path not in RAW_RUNTIME_CODEC_EXEMPTIONS
        and re.search(
            r"\b(?:BufMut|BytesMut|ValueEncoder|ValueDecoder)\b"
            r"|\.put_(?:u8|u16|u32|u64|f32|slice)\s*\(",
            text,
        )
    ):
        errors.append(f"{path}: raw managed-index serialization must use encoding::v2")
    if path.endswith("encoding/v2/values/lifecycle.rs"):
        for dependency in (
            "decode_secondary_entry",
            "encode_secondary_entry",
            "values::indexes",
        ):
            if dependency in text:
                errors.append(
                    f"{path}: lifecycle values depend on index dispatch {dependency!r}"
                )
    return errors


def validate_codec_architecture(root: Path) -> list[str]:
    errors = []
    source_root = root / "crates" / "db" / "src"
    for path in source_root.rglob("*.rs"):
        errors.extend(
            forbidden_codec_source(path.relative_to(root).as_posix(), path.read_text())
        )
    v2 = source_root / "encoding" / "v2"
    for relative_path, owner in CODEC_OWNERS.items():
        path = v2 / relative_path
        if not path.is_file():
            errors.append(f"{path.relative_to(root)}: required codec module is missing")
            continue
        if owner not in path.read_text():
            errors.append(
                f"{path.relative_to(root)}: codec module does not own {owner!r}"
            )
    for old_path in (
        source_root / "encoding" / "v1" / "keys" / "index_v2.rs",
        source_root / "encoding" / "v1" / "values" / "index_v2",
        source_root / "index_v2",
    ):
        if old_path.exists():
            errors.append(f"{old_path.relative_to(root)}: obsolete codec/runtime path remains")
    for module in (
        v2 / "keys" / "indexes" / "mod.rs",
        v2 / "values" / "indexes" / "mod.rs",
    ):
        text = module.read_text()
        for family in ("equality", "range", "text", "vector"):
            if re.search(rf"\bmod\s+{family}\s*;", text) is None:
                errors.append(
                    f"{module.relative_to(root)}: missing {family!r} codec module declaration"
                )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-test-filters", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    catalog = load_catalog(root)
    errors = []
    errors.extend(validate_references(root, catalog, automation_files(root)))
    errors.extend(validate_fuzz_matrices(root, catalog))
    errors.extend(validate_inventory(root, catalog))
    errors.extend(validate_codec_architecture(root))
    if not args.skip_test_filters:
        errors.extend(validate_test_filters(root, catalog))
    if errors:
        print("Cargo target-reference validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Cargo references and managed-index codec boundaries are valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
