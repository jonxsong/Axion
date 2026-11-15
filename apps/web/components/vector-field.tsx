const points = [
  [10, 48, 2.4],
  [22, 20, 1.7],
  [30, 68, 2.2],
  [42, 38, 1.6],
  [55, 17, 2],
  [61, 58, 2.7],
  [75, 32, 1.7],
  [86, 64, 2.2],
  [92, 15, 1.5],
];

export function VectorField() {
  return (
    <div className="vector-field" aria-label="Abstract high-dimensional nearest-neighbor graph">
      <div className="field-grid" />
      <svg viewBox="0 0 100 82" preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <linearGradient id="edge" x1="0" x2="1">
            <stop stopColor="#6ee7b7" stopOpacity=".12" />
            <stop offset=".55" stopColor="#6ee7b7" stopOpacity=".75" />
            <stop offset="1" stopColor="#60a5fa" stopOpacity=".15" />
          </linearGradient>
        </defs>
        {points.slice(0, -1).map((point, index) => {
          const next = points[index + 1];
          const alternate = points[(index + 3) % points.length];
          return (
            <g key={`${point[0]}-${point[1]}`}>
              <line x1={point[0]} y1={point[1]} x2={next[0]} y2={next[1]} stroke="url(#edge)" />
              <line
                x1={point[0]}
                y1={point[1]}
                x2={alternate[0]}
                y2={alternate[1]}
                stroke="url(#edge)"
                opacity=".38"
              />
            </g>
          );
        })}
        {points.map(([x, y, radius], index) => (
          <g key={`${x}-${y}`} className={index === 5 ? "query-node" : "graph-node"}>
            {index === 5 && <circle cx={x} cy={y} r="8" className="node-ring" />}
            <circle cx={x} cy={y} r={radius} />
          </g>
        ))}
      </svg>
      <div className="field-label query-label">query</div>
      <div className="field-label result-label">nearest / 0.982</div>
    </div>
  );
}

