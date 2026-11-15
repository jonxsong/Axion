# Axion Python client

Python 3.12+ typed sync and async clients for Axion's REST API.

```python
from axion_client import AxionClient

with AxionClient("http://localhost:8080") as client:
    client.create_collection("documents", dimensions=384, metric="cosine")
    client.upsert(
        "documents",
        [
            {
                "id": "doc-1",
                "values": [0.1] * 384,
                "metadata": {"category": "engineering"},
            }
        ],
    )
    response = client.search("documents", [0.1] * 384, top_k=5)
    print(response.results)
```

Set `api_key=` for protected deployments. The client retries transport failures, 429, 502, 503, and 504 only for operations with safe retry semantics (reads, search, idempotent upserts, and deletes). Collection creation is never automatically retried.

