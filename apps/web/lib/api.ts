export type Metric = "cosine" | "euclidean" | "dot";
export type SearchMode = "exact" | "approximate";

export interface HnswConfig {
  m: number;
  ef_construction: number;
  ef_search: number;
  random_seed: number;
}

export interface Collection {
  name: string;
  dimensions: number;
  metric: Metric;
  hnsw: HnswConfig;
  vector_count: number;
}

export interface VectorRecord {
  id: string;
  values: number[];
  metadata: Record<string, unknown>;
}

export interface SearchResult {
  id: string;
  score: number;
  metadata: Record<string, unknown>;
}

export interface SearchResponse {
  results: SearchResult[];
  took_ms: number;
}

export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly code: string,
  ) {
    super(message);
  }
}

export async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
  apiKey?: string,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...options,
    headers: {
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...(apiKey ? { Authorization: `Bearer ${apiKey}` } : {}),
      ...options.headers,
    },
  });
  if (!response.ok) {
    const body = await response.json().catch(() => null);
    throw new ApiError(
      body?.error?.message ?? response.statusText,
      response.status,
      body?.error?.code ?? "http_error",
    );
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export async function fetchMetrics(apiKey?: string): Promise<string> {
  const response = await fetch("/api/metrics", {
    headers: apiKey ? { Authorization: `Bearer ${apiKey}` } : undefined,
  });
  if (!response.ok) throw new ApiError("Metrics unavailable", response.status, "metrics_error");
  return response.text();
}

export function metricTotal(text: string, prefix: string): number {
  return text
    .split("\n")
    .filter((line) => line.startsWith(prefix) && !line.startsWith("#"))
    .reduce((sum, line) => sum + Number(line.trim().split(/\s+/).at(-1) ?? 0), 0);
}

