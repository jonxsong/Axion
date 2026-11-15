const canvas = document.querySelector("#vector-canvas");
const context = canvas?.getContext("2d");

const baseNodes = [
  [.14, .22], [.27, .14], [.42, .23], [.63, .15], [.81, .24], [.19, .42], [.35, .38],
  [.53, .43], [.72, .38], [.88, .47], [.12, .65], [.31, .61], [.48, .69], [.66, .61],
  [.82, .72], [.24, .84], [.56, .85], [.76, .88],
];
const edges = [[0,1],[0,5],[1,2],[1,6],[2,3],[2,6],[2,7],[3,4],[3,7],[3,8],[4,8],[5,6],[5,10],[5,11],[6,7],[6,11],[7,8],[7,12],[7,13],[8,9],[8,13],[9,14],[10,11],[10,15],[11,12],[11,15],[12,13],[12,16],[13,14],[13,16],[13,17],[14,17],[15,16],[16,17]];
let pointer = { x: .58, y: .58 };

function sizeCanvas() {
  if (!canvas || !context) return;
  const ratio = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  canvas.width = Math.max(1, Math.floor(rect.width * ratio));
  canvas.height = Math.max(1, Math.floor(rect.height * ratio));
  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  drawGraph();
}

function drawGraph() {
  if (!canvas || !context) return;
  const { width, height } = canvas.getBoundingClientRect();
  context.clearRect(0, 0, width, height);
  const nodes = baseNodes.map(([x, y], index) => {
    const wave = Math.sin(Date.now() / 1500 + index) * 2;
    return [x * width + wave, y * height + Math.cos(index) * 2];
  });

  context.lineWidth = 1;
  edges.forEach(([a, b]) => {
    context.beginPath();
    context.moveTo(...nodes[a]);
    context.lineTo(...nodes[b]);
    context.strokeStyle = "rgba(148, 155, 165, .16)";
    context.stroke();
  });

  const query = [pointer.x * width, pointer.y * height];
  const nearest = nodes
    .map((node, index) => ({ node, index, distance: Math.hypot(node[0] - query[0], node[1] - query[1]) }))
    .sort((a, b) => a.distance - b.distance)
    .slice(0, 4);

  nearest.forEach(({ node }, rank) => {
    context.beginPath();
    context.moveTo(...query);
    context.lineTo(...node);
    context.strokeStyle = `rgba(110, 231, 183, ${.52 - rank * .08})`;
    context.stroke();
  });

  nodes.forEach((node, index) => {
    const selected = nearest.some((item) => item.index === index);
    context.beginPath();
    context.arc(node[0], node[1], selected ? 5 : 3.5, 0, Math.PI * 2);
    context.fillStyle = selected ? "#79aef8" : "rgba(148, 155, 165, .82)";
    context.shadowColor = selected ? "rgba(121, 174, 248, .7)" : "transparent";
    context.shadowBlur = selected ? 11 : 0;
    context.fill();
  });

  context.shadowColor = "rgba(110, 231, 183, .9)";
  context.shadowBlur = 18;
  context.beginPath();
  context.arc(query[0], query[1], 7, 0, Math.PI * 2);
  context.fillStyle = "#6ee7b7";
  context.fill();
  context.shadowBlur = 0;
  context.beginPath();
  context.arc(query[0], query[1], 16, 0, Math.PI * 2);
  context.strokeStyle = "rgba(110, 231, 183, .28)";
  context.stroke();
}

canvas?.addEventListener("pointermove", (event) => {
  const rect = canvas.getBoundingClientRect();
  pointer = {
    x: Math.min(.94, Math.max(.06, (event.clientX - rect.left) / rect.width)),
    y: Math.min(.92, Math.max(.10, (event.clientY - rect.top) / rect.height)),
  };
  drawGraph();
});

window.addEventListener("resize", sizeCanvas);
sizeCanvas();

if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
  window.setInterval(drawGraph, 220);
}

const observer = new IntersectionObserver((entries) => {
  entries.forEach((entry) => {
    if (entry.isIntersecting) {
      entry.target.classList.add("visible");
      observer.unobserve(entry.target);
    }
  });
}, { threshold: .12 });
document.querySelectorAll(".reveal").forEach((element) => observer.observe(element));

const tabs = [...document.querySelectorAll(".console-tabs button")];
const consoleImage = document.querySelector("#console-image");
tabs.forEach((tab) => {
  tab.addEventListener("click", () => {
    tabs.forEach((item) => item.setAttribute("aria-selected", String(item === tab)));
    if (!consoleImage) return;
    consoleImage.style.opacity = "0";
    window.setTimeout(() => {
      consoleImage.src = tab.dataset.image;
      consoleImage.alt = tab.dataset.alt;
      consoleImage.style.opacity = "1";
    }, 140);
  });
});

document.querySelector("#copy-code")?.addEventListener("click", async (event) => {
  const button = event.currentTarget;
  const code = document.querySelector("#quickstart-code")?.textContent ?? "";
  await navigator.clipboard.writeText(code);
  button.textContent = "Copied";
  window.setTimeout(() => { button.textContent = "Copy"; }, 1400);
});
