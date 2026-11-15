from __future__ import annotations

import httpx
import pytest

from axion_client import (
    AsyncAxionClient,
    AxionAPIError,
    AxionClient,
    AxionNotFoundError,
    AxionValidationError,
)


def test_sync_client_serializes_models_auth_and_search() -> None:
    requests: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        requests.append(request)
        if request.url.path == "/v1/collections" and request.method == "POST":
            return httpx.Response(
                201,
                json={
                    "name": "documents",
                    "dimensions": 2,
                    "metric": "cosine",
                    "hnsw": {
                        "m": 16,
                        "ef_construction": 200,
                        "ef_search": 64,
                        "random_seed": 2_802_843_649,
                    },
                    "vector_count": 0,
                },
            )
        if request.url.path.endswith("/vectors/batch"):
            return httpx.Response(204)
        if request.url.path.endswith("/search"):
            return httpx.Response(
                200,
                json={
                    "results": [
                        {"id": "doc-1", "score": 1.0, "metadata": {"category": "research"}}
                    ],
                    "took_ms": 0.42,
                },
            )
        raise AssertionError(f"unexpected request: {request.method} {request.url}")

    with AxionClient(
        api_key="test-key", transport=httpx.MockTransport(handler), backoff_seconds=0
    ) as client:
        collection = client.create_collection("documents", 2)
        client.upsert(
            "documents",
            [{"id": "doc-1", "values": [1.0, 0.0], "metadata": {"category": "research"}}],
        )
        result = client.search(
            "documents",
            [1.0, 0.0],
            filter={"eq": {"field": "category", "value": "research"}},
        )

    assert collection.name == "documents"
    assert result.results[0].id == "doc-1"
    assert all(request.headers["authorization"] == "Bearer test-key" for request in requests)


def test_retries_idempotent_requests_but_not_create() -> None:
    attempts = 0

    def handler(_: httpx.Request) -> httpx.Response:
        nonlocal attempts
        attempts += 1
        if attempts < 3:
            return httpx.Response(503, json={"error": {"code": "busy", "message": "retry"}})
        return httpx.Response(200, json=[])

    with AxionClient(
        transport=httpx.MockTransport(handler), max_attempts=3, backoff_seconds=0
    ) as client:
        assert client.list_collections() == []
    assert attempts == 3

    attempts = 0
    with (
        AxionClient(
            transport=httpx.MockTransport(handler), max_attempts=3, backoff_seconds=0
        ) as client,
        pytest.raises(AxionAPIError),
    ):
        client.create_collection("documents", 2)
    assert attempts == 1


def test_structured_errors_and_model_validation() -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        return httpx.Response(
            404, json={"error": {"code": "not_found", "message": "collection missing"}}
        )

    with AxionClient(transport=httpx.MockTransport(handler)) as client:
        with pytest.raises(AxionNotFoundError) as captured:
            client.get_collection("missing")
        assert captured.value.code == "not_found"
        with pytest.raises(ValueError):
            client.upsert("missing", [{"id": "bad", "values": [float("nan")]}])


@pytest.mark.asyncio
async def test_async_client_and_validation_error() -> None:
    async def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path.endswith("/search"):
            return httpx.Response(
                400, json={"error": {"code": "invalid_request", "message": "bad"}}
            )
        return httpx.Response(200, json=[])

    async with AsyncAxionClient(
        transport=httpx.MockTransport(handler), backoff_seconds=0
    ) as client:
        assert await client.list_collections() == []
        with pytest.raises(AxionValidationError):
            await client.search("documents", [1.0], top_k=0)
