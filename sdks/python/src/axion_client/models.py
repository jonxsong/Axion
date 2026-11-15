"""Pydantic request and response models for Axion v1."""

from __future__ import annotations

import math
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator

DistanceMetric = Literal["cosine", "euclidean", "dot"]
SearchMode = Literal["exact", "approximate"]


class HnswConfig(BaseModel):
    """HNSW construction and query defaults."""

    model_config = ConfigDict(extra="forbid")

    m: int = Field(default=16, ge=2)
    ef_construction: int = Field(default=200, ge=2)
    ef_search: int = Field(default=64, ge=1)
    random_seed: int = Field(default=2_802_843_649, ge=0)


class CollectionCreate(BaseModel):
    """Collection creation payload."""

    model_config = ConfigDict(extra="forbid")

    name: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9_-]+$")
    dimensions: int = Field(gt=0)
    metric: DistanceMetric = "cosine"
    hnsw: HnswConfig = Field(default_factory=HnswConfig)


class Collection(CollectionCreate):
    """Persisted collection and live vector count."""

    vector_count: int = Field(default=0, ge=0)


class Vector(BaseModel):
    """Dense vector record."""

    model_config = ConfigDict(extra="forbid")

    id: str = Field(min_length=1, max_length=512)
    values: list[float] = Field(min_length=1)
    metadata: dict[str, Any] = Field(default_factory=dict)

    @field_validator("values")
    @classmethod
    def finite_values(cls, values: list[float]) -> list[float]:
        if any(not math.isfinite(value) for value in values):
            raise ValueError("vector values must be finite numbers")
        return values


class SearchResult(BaseModel):
    """One scored neighbor; higher scores are always better."""

    id: str
    score: float
    metadata: dict[str, Any] = Field(default_factory=dict)


class SearchResponse(BaseModel):
    """Neighbors and measured server-side request latency."""

    results: list[SearchResult]
    took_ms: float = Field(ge=0)


class EqFilter(BaseModel):
    """Equality expression for one dot-separated metadata field."""

    field: str = Field(min_length=1)
    value: Any


Filter = dict[str, Any]
