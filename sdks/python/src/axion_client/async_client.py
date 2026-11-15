"""Asynchronous Axion client."""

from __future__ import annotations

import asyncio
from collections.abc import Iterable
from typing import Any

import httpx

from .client import _RETRY_STATUS, _api_error
from .exceptions import AxionTransportError
from .models import (
    Collection,
    CollectionCreate,
    DistanceMetric,
    Filter,
    SearchMode,
    SearchResponse,
    Vector,
)


class AsyncAxionClient:
    """Async client for the Axion REST v1 API."""

    def __init__(
        self,
        url: str = "http://localhost:8080",
        *,
        api_key: str | None = None,
        timeout: float | httpx.Timeout = 10.0,
        max_attempts: int = 3,
        backoff_seconds: float = 0.1,
        transport: httpx.AsyncBaseTransport | None = None,
    ) -> None:
        if max_attempts < 1:
            raise ValueError("max_attempts must be at least 1")
        headers = {"Authorization": f"Bearer {api_key}"} if api_key else None
        self._client = httpx.AsyncClient(
            base_url=url.rstrip("/"),
            headers=headers,
            timeout=timeout,
            transport=transport,
        )
        self._max_attempts = max_attempts
        self._backoff_seconds = backoff_seconds

    async def __aenter__(self) -> AsyncAxionClient:
        return self

    async def __aexit__(self, *_: object) -> None:
        await self.close()

    async def close(self) -> None:
        await self._client.aclose()

    async def _request(
        self, method: str, path: str, *, retryable: bool, **kwargs: Any
    ) -> httpx.Response:
        attempts = self._max_attempts if retryable else 1
        last_error: httpx.TransportError | None = None
        for attempt in range(attempts):
            try:
                response = await self._client.request(method, path, **kwargs)
            except httpx.TransportError as error:
                last_error = error
            else:
                if response.status_code not in _RETRY_STATUS or attempt + 1 == attempts:
                    if response.is_error:
                        raise _api_error(response)
                    return response
            if attempt + 1 < attempts:
                await asyncio.sleep(self._backoff_seconds * 2**attempt)
        raise AxionTransportError(f"request failed after {attempts} attempts") from last_error

    async def create_collection(
        self,
        name: str,
        dimensions: int,
        metric: DistanceMetric = "cosine",
        *,
        hnsw: dict[str, Any] | None = None,
    ) -> Collection:
        payload = CollectionCreate(
            name=name,
            dimensions=dimensions,
            metric=metric,
            hnsw=hnsw or {},
        )
        response = await self._request(
            "POST",
            "/v1/collections",
            retryable=False,
            json=payload.model_dump(mode="json"),
        )
        return Collection.model_validate(response.json())

    async def list_collections(self) -> list[Collection]:
        response = await self._request("GET", "/v1/collections", retryable=True)
        return [Collection.model_validate(item) for item in response.json()]

    async def get_collection(self, name: str) -> Collection:
        response = await self._request("GET", f"/v1/collections/{name}", retryable=True)
        return Collection.model_validate(response.json())

    async def delete_collection(self, name: str) -> None:
        await self._request("DELETE", f"/v1/collections/{name}", retryable=True)

    async def upsert(self, collection: str, vectors: Iterable[Vector | dict[str, Any]]) -> None:
        validated = [Vector.model_validate(vector) for vector in vectors]
        await self._request(
            "POST",
            f"/v1/collections/{collection}/vectors/batch",
            retryable=True,
            json={"vectors": [vector.model_dump(mode="json") for vector in validated]},
        )

    async def get_vector(self, collection: str, vector_id: str) -> Vector:
        response = await self._request(
            "GET", f"/v1/collections/{collection}/vectors/{vector_id}", retryable=True
        )
        return Vector.model_validate(response.json())

    async def list_vectors(
        self, collection: str, *, after: str | None = None, limit: int = 100
    ) -> list[Vector]:
        response = await self._request(
            "GET",
            f"/v1/collections/{collection}/vectors",
            retryable=True,
            params={"after": after, "limit": limit},
        )
        return [Vector.model_validate(item) for item in response.json()]

    async def delete_vector(self, collection: str, vector_id: str) -> None:
        await self._request(
            "DELETE", f"/v1/collections/{collection}/vectors/{vector_id}", retryable=True
        )

    async def search(
        self,
        collection: str,
        vector: list[float],
        *,
        top_k: int = 10,
        mode: SearchMode = "approximate",
        filter: Filter | None = None,
        ef_search: int | None = None,
    ) -> SearchResponse:
        payload = {
            "vector": vector,
            "top_k": top_k,
            "mode": mode,
            "filter": filter,
            "ef_search": ef_search,
        }
        response = await self._request(
            "POST",
            f"/v1/collections/{collection}/search",
            retryable=True,
            json={key: value for key, value in payload.items() if value is not None},
        )
        return SearchResponse.model_validate(response.json())
