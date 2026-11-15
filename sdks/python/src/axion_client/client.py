"""Synchronous Axion client."""

from __future__ import annotations

import time
from collections.abc import Iterable
from typing import Any

import httpx

from .exceptions import (
    AxionAPIError,
    AxionConflictError,
    AxionNotFoundError,
    AxionTransportError,
    AxionValidationError,
)
from .models import (
    Collection,
    CollectionCreate,
    DistanceMetric,
    Filter,
    SearchMode,
    SearchResponse,
    Vector,
)

_RETRY_STATUS = {429, 502, 503, 504}


def _api_error(response: httpx.Response) -> AxionAPIError:
    try:
        detail = response.json()["error"]
        code = str(detail["code"])
        message = str(detail["message"])
    except (KeyError, TypeError, ValueError):
        code = "http_error"
        message = response.text or response.reason_phrase
    error_type = {
        400: AxionValidationError,
        404: AxionNotFoundError,
        409: AxionConflictError,
    }.get(response.status_code, AxionAPIError)
    return error_type(response.status_code, code, message)


class AxionClient:
    """Blocking client for the Axion REST v1 API."""

    def __init__(
        self,
        url: str = "http://localhost:8080",
        *,
        api_key: str | None = None,
        timeout: float | httpx.Timeout = 10.0,
        max_attempts: int = 3,
        backoff_seconds: float = 0.1,
        transport: httpx.BaseTransport | None = None,
    ) -> None:
        if max_attempts < 1:
            raise ValueError("max_attempts must be at least 1")
        headers = {"Authorization": f"Bearer {api_key}"} if api_key else None
        self._client = httpx.Client(
            base_url=url.rstrip("/"),
            headers=headers,
            timeout=timeout,
            transport=transport,
        )
        self._max_attempts = max_attempts
        self._backoff_seconds = backoff_seconds

    def __enter__(self) -> AxionClient:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def close(self) -> None:
        self._client.close()

    def _request(self, method: str, path: str, *, retryable: bool, **kwargs: Any) -> httpx.Response:
        attempts = self._max_attempts if retryable else 1
        last_error: httpx.TransportError | None = None
        for attempt in range(attempts):
            try:
                response = self._client.request(method, path, **kwargs)
            except httpx.TransportError as error:
                last_error = error
            else:
                if response.status_code not in _RETRY_STATUS or attempt + 1 == attempts:
                    if response.is_error:
                        raise _api_error(response)
                    return response
            if attempt + 1 < attempts:
                time.sleep(self._backoff_seconds * 2**attempt)
        raise AxionTransportError(f"request failed after {attempts} attempts") from last_error

    def create_collection(
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
        response = self._request(
            "POST",
            "/v1/collections",
            retryable=False,
            json=payload.model_dump(mode="json"),
        )
        return Collection.model_validate(response.json())

    def list_collections(self) -> list[Collection]:
        response = self._request("GET", "/v1/collections", retryable=True)
        return [Collection.model_validate(item) for item in response.json()]

    def get_collection(self, name: str) -> Collection:
        response = self._request("GET", f"/v1/collections/{name}", retryable=True)
        return Collection.model_validate(response.json())

    def delete_collection(self, name: str) -> None:
        self._request("DELETE", f"/v1/collections/{name}", retryable=True)

    def upsert(self, collection: str, vectors: Iterable[Vector | dict[str, Any]]) -> None:
        validated = [Vector.model_validate(vector) for vector in vectors]
        self._request(
            "POST",
            f"/v1/collections/{collection}/vectors/batch",
            retryable=True,
            json={"vectors": [vector.model_dump(mode="json") for vector in validated]},
        )

    def get_vector(self, collection: str, vector_id: str) -> Vector:
        response = self._request(
            "GET", f"/v1/collections/{collection}/vectors/{vector_id}", retryable=True
        )
        return Vector.model_validate(response.json())

    def list_vectors(
        self, collection: str, *, after: str | None = None, limit: int = 100
    ) -> list[Vector]:
        response = self._request(
            "GET",
            f"/v1/collections/{collection}/vectors",
            retryable=True,
            params={"after": after, "limit": limit},
        )
        return [Vector.model_validate(item) for item in response.json()]

    def delete_vector(self, collection: str, vector_id: str) -> None:
        self._request("DELETE", f"/v1/collections/{collection}/vectors/{vector_id}", retryable=True)

    def search(
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
        response = self._request(
            "POST",
            f"/v1/collections/{collection}/search",
            retryable=True,
            json={key: value for key, value in payload.items() if value is not None},
        )
        return SearchResponse.model_validate(response.json())
