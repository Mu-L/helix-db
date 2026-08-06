#!/usr/bin/env python3
"""Prepare a pinned DBpedia 1536-dimensional fbin benchmark fixture."""

import argparse
import hashlib
import shutil
import ssl
import struct
import tempfile
import urllib.request
from pathlib import Path

import certifi
import pyarrow.parquet as parquet


DATASET = "Qdrant/dbpedia-entities-openai3-text-embedding-3-large-1536-1M"
REVISION = "4a9731217921bc476a0f03544f11f22ae4903fa5"
SHARD_COUNT = 26
SHARDS = tuple(
    f"train-{shard_index:05d}-of-{SHARD_COUNT:05d}.parquet"
    for shard_index in range(SHARD_COUNT)
)
COLUMN = "text-embedding-3-large-1536-embedding"
DEFAULT_ROW_COUNT = 50_000
FULL_ROW_COUNT = 1_000_000
DIMENSION = 1_536
EXPECTED_50K_SHA256 = (
    "43a6b640d8b10a0e32a102ebade0d50bc5b526f52d485e6a4117eff93d59253e"
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(8 * 1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--rows",
        choices=(DEFAULT_ROW_COUNT, FULL_ROW_COUNT),
        default=DEFAULT_ROW_COUNT,
        type=int,
    )
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    partial_output = args.output.with_suffix(f"{args.output.suffix}.partial")
    partial_output.unlink(missing_ok=True)

    written = 0
    tls_context = ssl.create_default_context(cafile=certifi.where())
    try:
        with tempfile.TemporaryDirectory(prefix="helix-dbpedia-") as temporary:
            temporary = Path(temporary)
            with partial_output.open("wb") as sink:
                sink.write(struct.pack("<II", args.rows, DIMENSION))
                for shard in SHARDS:
                    source = temporary / shard
                    url = (
                        f"https://huggingface.co/datasets/{DATASET}/resolve/"
                        f"{REVISION}/data/{shard}"
                    )
                    print(f"downloading {shard}", flush=True)
                    with urllib.request.urlopen(url, context=tls_context) as response:
                        with source.open("wb") as source_file:
                            shutil.copyfileobj(response, source_file)
                    for batch in parquet.ParquetFile(source).iter_batches(
                        batch_size=512, columns=[COLUMN]
                    ):
                        remaining = args.rows - written
                        if remaining == 0:
                            break
                        vectors = batch.column(0)
                        rows = min(len(vectors), remaining)
                        values = vectors.values.to_numpy(zero_copy_only=False).reshape(
                            len(vectors), DIMENSION
                        )
                        values[:rows].astype("<f4", copy=False).tofile(sink)
                        written += rows
                    source.unlink()
                    print(f"prepared_rows={written}", flush=True)
                    if written == args.rows:
                        break

        if written != args.rows:
            raise RuntimeError(f"expected {args.rows} rows, wrote {written}")
        digest = sha256(partial_output)
        if args.rows == DEFAULT_ROW_COUNT and digest != EXPECTED_50K_SHA256:
            raise RuntimeError(f"unexpected fixture SHA-256: {digest}")
        partial_output.replace(args.output)
        print(f"{args.output}: {args.rows}x{DIMENSION}, sha256={digest}")
    except BaseException:
        partial_output.unlink(missing_ok=True)
        raise


if __name__ == "__main__":
    main()
