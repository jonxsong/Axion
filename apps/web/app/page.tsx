import {
  Activity,
  ArrowRight,
  Boxes,
  Braces,
  Code2,
  Database,
  Gauge,
  GitBranch,
  Network,
  ShieldCheck,
} from "lucide-react";
import Link from "next/link";

import { Logo } from "@/components/logo";
import { VectorField } from "@/components/vector-field";

const features = [
  [Network, "Native HNSW", "A multilayer proximity graph implemented in Rust, with explicit recall and search-width controls."],
  [Database, "Durable by default", "Synchronous RocksDB write batches and deterministic index reconstruction after restart."],
  [GitBranch, "Metadata-aware", "Boolean filters with documented exact pre-filter semantics and nested JSON fields."],
  [Braces, "REST, gRPC, Python", "One engine, versioned contracts, typed clients, and consistent higher-is-better scores."],
  [Gauge, "Measured performance", "Criterion microbenchmarks and official ANN-Benchmarks integration—never invented numbers."],
  [Activity, "Observable", "Request IDs, structured logs, Prometheus counters, and latency histograms."],
] as const;

export default function Home() {
  return (
    <main>
      <header className="site-nav shell">
        <Logo />
        <nav aria-label="Primary navigation">
          <a href="#architecture">Architecture</a>
          <a href="#developers">Developers</a>
          <Link href="/api/docs">Docs</Link>
        </nav>
        <div className="nav-actions">
          <a className="icon-link" href="https://github.com/jonxsong/Axion" aria-label="Axion on GitHub">
            <Code2 size={18} />
          </a>
          <Link className="button button-small" href="/console">
            Open console
          </Link>
        </div>
      </header>

      <section className="hero shell">
        <div className="hero-copy">
          <div className="eyebrow"><span /> Open-source vector infrastructure</div>
          <h1>Vector search,<br /><em>engineered</em> from first principles.</h1>
          <p>
            A high-performance vector search engine built in Rust. Designed for fast similarity
            search, durable storage, and developer control.
          </p>
          <div className="hero-actions">
            <Link className="button" href="#quickstart">Get started <ArrowRight size={16} /></Link>
            <a className="button button-secondary" href="https://github.com/jonxsong/Axion">
              <Code2 size={16} /> View on GitHub
            </a>
          </div>
          <div className="hero-proof">
            <span><ShieldCheck size={15} /> Synchronous WAL</span>
            <span><Boxes size={15} /> Rust-native engine</span>
          </div>
        </div>
        <VectorField />
      </section>

      <section className="principles shell" aria-label="Engineering principles">
        <p>BUILT FOR THE PARTS THAT MATTER</p>
        <div><span>01</span> Predictable latency</div>
        <div><span>02</span> Explicit trade-offs</div>
        <div><span>03</span> Durable state</div>
        <div><span>04</span> Open internals</div>
      </section>

      <section className="section shell" id="architecture">
        <div className="section-heading">
          <div><span className="section-index">01 / ARCHITECTURE</span><h2>A short path from vector to answer.</h2></div>
          <p>Every layer has one job. APIs validate. The query engine plans. Rust indexes search. RocksDB owns durable truth.</p>
        </div>
        <div className="pipeline">
          {["Clients", "API boundary", "Query planner", "Exact / HNSW", "RocksDB"].map((label, index) => (
            <div className="pipeline-step" key={label}>
              <span>{String(index + 1).padStart(2, "0")}</span>
              <strong>{label}</strong>
              {index < 4 && <ArrowRight size={17} />}
            </div>
          ))}
        </div>
      </section>

      <section className="section shell">
        <div className="section-heading compact-heading">
          <div><span className="section-index">02 / CAPABILITIES</span><h2>Infrastructure, not theater.</h2></div>
          <p>Features are only shown here when they exist in the engine and have an executable validation path.</p>
        </div>
        <div className="feature-grid">
          {features.map(([Icon, title, description]) => (
            <article key={title}>
              <Icon size={21} />
              <h3>{title}</h3>
              <p>{description}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section shell developer-section" id="developers">
        <div className="dev-copy">
          <span className="section-index">03 / DEVELOPER EXPERIENCE</span>
          <h2>Useful in four calls.</h2>
          <p>The Python client validates requests locally, supports sync and async workflows, and preserves structured metadata.</p>
          <Link href="/console">Inspect the live console <ArrowRight size={16} /></Link>
        </div>
        <div className="code-window" id="quickstart">
          <div className="code-title"><span /><span /><span /><b>quickstart.py</b></div>
          <pre><code>{`from axion_client import AxionClient

client = AxionClient("http://localhost:8080")
client.create_collection(
    "documents", dimensions=384, metric="cosine"
)
client.upsert("documents", [{
    "id": "doc-1", "values": embedding,
    "metadata": {"team": "search"}
}])
results = client.search("documents", query, top_k=10)`}</code></pre>
        </div>
      </section>

      <section className="section shell benchmark-note">
        <div><span className="section-index">04 / PERFORMANCE</span><h2>Numbers earn their place.</h2></div>
        <p>Axion publishes benchmark charts only after the exact run artifacts, hardware profile, dataset, and configuration are committed. Use the benchmark guide to reproduce Criterion and ANN-Benchmarks locally.</p>
        <Link className="button button-secondary" href="/api/docs">Read API docs <ArrowRight size={16} /></Link>
      </section>

      <footer className="shell">
        <Logo />
        <p>Vector search, engineered from first principles.</p>
        <div><a href="https://github.com/jonxsong/Axion">GitHub</a><Link href="/api/docs">Documentation</Link><a href="https://github.com/jonxsong/Axion/blob/main/LICENSE">MIT License</a></div>
      </footer>
    </main>
  );
}

