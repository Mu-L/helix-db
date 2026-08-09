# helix-db-embedded

Native embedded runtime for the `helix-db` Python SDK.

Install matching package versions:

```sh
python -m pip install "helix-db==0.3.2" "helix-db-embedded==0.3.2"
```

Applications continue to use the public SDK:

```python
from helixdb import Client, Disk

client = Client.embedded(Disk("./data", "app"))
```

The SDK imports `helixdb_uniffi` internally. Native wheels are published for
macOS universal2, manylinux x86_64/aarch64, and Windows x86_64.
