import type { VectorRecord } from "./api";

export interface Point2D {
  id: string;
  x: number;
  y: number;
}

function normalize(vector: number[]): number[] {
  const length = Math.sqrt(vector.reduce((sum, value) => sum + value * value, 0));
  return length === 0 ? vector.map(() => 0) : vector.map((value) => value / length);
}

function component(rows: number[][], orthogonal?: number[]): number[] {
  const dimensions = rows[0]?.length ?? 0;
  let direction = normalize(Array.from({ length: dimensions }, (_, index) => 1 + (index % 7)));
  for (let iteration = 0; iteration < 32; iteration += 1) {
    const next = Array.from({ length: dimensions }, () => 0);
    for (const row of rows) {
      const projection = row.reduce((sum, value, index) => sum + value * direction[index], 0);
      row.forEach((value, index) => (next[index] += value * projection));
    }
    if (orthogonal) {
      const overlap = next.reduce((sum, value, index) => sum + value * orthogonal[index], 0);
      next.forEach((_, index) => (next[index] -= overlap * orthogonal[index]));
    }
    direction = normalize(next);
  }
  return direction;
}

export function projectPca(records: VectorRecord[]): Point2D[] {
  if (records.length === 0) return [];
  const dimensions = records[0].values.length;
  const means = Array.from({ length: dimensions }, (_, index) =>
    records.reduce((sum, record) => sum + record.values[index], 0) / records.length,
  );
  const centered = records.map((record) => record.values.map((value, index) => value - means[index]));
  const first = component(centered);
  const second = dimensions > 1 ? component(centered, first) : first.map(() => 0);
  const raw = centered.map((row, index) => ({
    id: records[index].id,
    x: row.reduce((sum, value, offset) => sum + value * first[offset], 0),
    y: row.reduce((sum, value, offset) => sum + value * second[offset], 0),
  }));
  const xs = raw.map((point) => point.x);
  const ys = raw.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  return raw.map((point) => ({
    ...point,
    x: 6 + ((point.x - minX) / (maxX - minX || 1)) * 88,
    y: 6 + ((point.y - minY) / (maxY - minY || 1)) * 88,
  }));
}

