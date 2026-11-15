import { describe, expect, it } from "vitest";

import { metricTotal } from "../lib/api";
import { projectPca } from "../lib/pca";

describe("PCA projection", () => {
  it("returns stable bounded points for vector records", () => {
    const points = projectPca([
      { id: "a", values: [1, 0, 0], metadata: {} },
      { id: "b", values: [0, 1, 0], metadata: {} },
      { id: "c", values: [0, 0, 1], metadata: {} },
    ]);
    expect(points.map((point) => point.id)).toEqual(["a", "b", "c"]);
    expect(points.every((point) => point.x >= 6 && point.x <= 94)).toBe(true);
    expect(points.every((point) => point.y >= 6 && point.y <= 94)).toBe(true);
  });
});

describe("Prometheus parsing", () => {
  it("sums all labeled series", () => {
    const text = [
      '# HELP axion_searches_total searches',
      'axion_searches_total{mode="exact"} 2',
      'axion_searches_total{mode="approximate"} 5',
    ].join("\n");
    expect(metricTotal(text, "axion_searches_total")).toBe(7);
  });
});

