"use client";

import { QueryClient, QueryClientProvider, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  BookOpen,
  Boxes,
  CircleGauge,
  Code2,
  Database,
  HeartPulse,
  LayoutDashboard,
  Plus,
  Search,
  Settings,
  SlidersHorizontal,
  Trash2,
  VectorSquare,
} from "lucide-react";
import { useMemo, useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import { Logo } from "@/components/logo";
import {
  type Collection,
  type Metric,
  type SearchResponse,
  type VectorRecord,
  apiFetch,
  fetchMetrics,
  metricTotal,
} from "@/lib/api";
import { projectPca } from "@/lib/pca";

const navigation = [
  ["Overview", LayoutDashboard],
  ["Collections", Database],
  ["Vector Explorer", VectorSquare],
  ["Search Playground", Search],
  ["Index Configuration", SlidersHorizontal],
  ["Performance", Activity],
  ["System Health", HeartPulse],
  ["API Documentation", BookOpen],
  ["Settings", Settings],
] as const;

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: 1, staleTime: 2_000 } } });

export default function ConsolePage() {
  return (
    <QueryClientProvider client={queryClient}>
      <Console />
    </QueryClientProvider>
  );
}

function Console() {
  const [section, setSection] = useState<(typeof navigation)[number][0]>("Overview");
  const [apiKey, setApiKey] = useState(() =>
    typeof window === "undefined" ? "" : (sessionStorage.getItem("axion-api-key") ?? ""),
  );
  const health = useQuery({
    queryKey: ["health"],
    queryFn: () => apiFetch<{ status: string }>("/health/ready", {}, apiKey),
    refetchInterval: 5_000,
    retry: false,
  });
  return (
    <div className="console-shell">
      <aside className="console-sidebar">
        <Logo />
        <nav className="console-nav" aria-label="Console navigation">
          {navigation.map(([label, Icon]) => (
            <button aria-label={label} key={label} className={section === label ? "active" : ""} onClick={() => setSection(label)}>
              <Icon size={16} /><span>{label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-status">
          <span className={`status-dot ${health.isSuccess ? "online" : ""}`} />
          <span>{health.isSuccess ? "Engine ready" : health.isLoading ? "Connecting" : "Unavailable"}</span>
        </div>
      </aside>
      <div className="console-main">
        <header className="console-topbar"><h1>{section}</h1><p>AXION / LOCAL</p></header>
        <div className="console-content">
          <Section section={section} apiKey={apiKey} setApiKey={setApiKey} health={health.data?.status} />
        </div>
      </div>
    </div>
  );
}

function useCollections(apiKey: string) {
  return useQuery({ queryKey: ["collections", apiKey], queryFn: () => apiFetch<Collection[]>("/v1/collections", {}, apiKey) });
}

function Section({ section, apiKey, setApiKey, health }: { section: string; apiKey: string; setApiKey: (value: string) => void; health?: string }) {
  switch (section) {
    case "Collections": return <Collections apiKey={apiKey} />;
    case "Vector Explorer": return <Explorer apiKey={apiKey} />;
    case "Search Playground": return <Playground apiKey={apiKey} />;
    case "Index Configuration": return <IndexConfiguration apiKey={apiKey} />;
    case "Performance": return <Performance apiKey={apiKey} />;
    case "System Health": return <Health apiKey={apiKey} health={health} />;
    case "API Documentation": return <Documentation />;
    case "Settings": return <SettingsPanel apiKey={apiKey} setApiKey={setApiKey} />;
    default: return <Overview apiKey={apiKey} health={health} />;
  }
}

function Heading({ title, description, children }: { title: string; description: string; children?: React.ReactNode }) {
  return <div className="panel-heading"><div><h2>{title}</h2><p>{description}</p></div>{children}</div>;
}

function ErrorMessage({ error }: { error: unknown }) {
  return error ? <div className="error-banner">{error instanceof Error ? error.message : "Request failed"}</div> : null;
}

function Overview({ apiKey, health }: { apiKey: string; health?: string }) {
  const collections = useCollections(apiKey);
  const total = collections.data?.reduce((sum, item) => sum + item.vector_count, 0) ?? 0;
  return <>
    <Heading title="System overview" description="Live collection and readiness state from this Axion node." />
    <ErrorMessage error={collections.error} />
    <div className="metric-grid">
      <Metric label="Collections" value={collections.data?.length ?? "—"} note="Durable namespaces" />
      <Metric label="Total vectors" value={collections.data ? total.toLocaleString() : "—"} note="Recovered live records" />
      <Metric label="Engine status" value={health === "ready" ? "Ready" : "Unavailable"} note="Storage-backed probe" />
      <Metric label="Index memory" value="Unavailable" note="Exporter not implemented" />
    </div>
    <div className="two-column">
      <div className="panel"><h3>Collections</h3>{collections.data?.length ? <table className="data-table"><tbody>{collections.data.slice(0, 5).map((item) => <tr key={item.name}><td><code>{item.name}</code></td><td>{item.vector_count.toLocaleString()} vectors</td><td>{item.metric}</td></tr>)}</tbody></table> : <Empty icon={Boxes} text="No collections yet" />}</div>
      <div className="panel"><h3>Telemetry availability</h3><p className="muted">Query throughput, latency histograms, and write counters are exported at <code className="mono">/metrics</code>. Storage bytes and index heap accounting are not yet exported and are shown honestly as unavailable.</p></div>
    </div>
  </>;
}

function Metric({ label, value, note }: { label: string; value: string | number; note: string }) {
  return <div className="metric-card"><span className="metric-label">{label}</span><strong>{value}</strong><small>{note}</small></div>;
}

function Empty({ icon: Icon, text }: { icon: typeof Boxes; text: string }) { return <div className="empty-state"><div><Icon size={24} /><span>{text}</span></div></div>; }

function Collections({ apiKey }: { apiKey: string }) {
  const query = useCollections(apiKey);
  const cache = useQueryClient();
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState({ name: "", dimensions: "384", metric: "cosine" as Metric, m: "16", ef: "200" });
  const create = useMutation({ mutationFn: () => apiFetch<Collection>("/v1/collections", { method: "POST", body: JSON.stringify({ name: form.name, dimensions: Number(form.dimensions), metric: form.metric, hnsw: { m: Number(form.m), ef_construction: Number(form.ef), ef_search: 64, random_seed: 2802843649 } }) }, apiKey), onSuccess: () => { setOpen(false); cache.invalidateQueries({ queryKey: ["collections"] }); } });
  const remove = useMutation({ mutationFn: (name: string) => apiFetch<void>(`/v1/collections/${name}`, { method: "DELETE" }, apiKey), onSuccess: () => cache.invalidateQueries({ queryKey: ["collections"] }) });
  return <>
    <Heading title="Collections" description="Create durable vector namespaces and inspect live configuration."><button className="button button-small" onClick={() => setOpen((value) => !value)}><Plus size={14} /> New collection</button></Heading>
    <ErrorMessage error={query.error ?? create.error ?? remove.error} />
    {open && <form className="panel form-grid" onSubmit={(event) => { event.preventDefault(); create.mutate(); }}>
      <Field label="Name"><input required pattern="[A-Za-z0-9_-]+" className="input" value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} /></Field>
      <Field label="Dimensions"><input required min="1" type="number" className="input" value={form.dimensions} onChange={(event) => setForm({ ...form, dimensions: event.target.value })} /></Field>
      <Field label="Metric"><select className="input" value={form.metric} onChange={(event) => setForm({ ...form, metric: event.target.value as Metric })}><option>cosine</option><option>euclidean</option><option>dot</option></select></Field>
      <Field label="HNSW M"><input min="2" type="number" className="input" value={form.m} onChange={(event) => setForm({ ...form, m: event.target.value })} /></Field>
      <Field label="ef construction"><input min="2" type="number" className="input" value={form.ef} onChange={(event) => setForm({ ...form, ef: event.target.value })} /></Field>
      <div className="field"><span aria-hidden="true">&nbsp;</span><button className="button" disabled={create.isPending}>Create durable collection</button></div>
    </form>}
    <div className="table-wrap"><table className="data-table"><thead><tr><th>Name</th><th>Dimensions</th><th>Metric</th><th>Vectors</th><th>HNSW</th><th>Actions</th></tr></thead><tbody>{query.data?.map((item) => <tr key={item.name}><td><code>{item.name}</code></td><td>{item.dimensions}</td><td>{item.metric}</td><td>{item.vector_count.toLocaleString()}</td><td>M {item.hnsw.m} / ef {item.hnsw.ef_search}</td><td><button aria-label={`Delete collection ${item.name}`} className="action-button danger" onClick={() => confirm(`Delete ${item.name} and all vectors?`) && remove.mutate(item.name)}><Trash2 size={12} /></button></td></tr>)}</tbody></table>{!query.isLoading && !query.data?.length && <Empty icon={Database} text="Create your first collection" />}</div>
  </>;
}

function Field({ label, children, full = false }: { label: string; children: React.ReactNode; full?: boolean }) { return <label className={`field ${full ? "full" : ""}`}><span>{label}</span>{children}</label>; }

function CollectionSelect({ collections, value, onChange }: { collections?: Collection[]; value: string; onChange: (value: string) => void }) { return <select aria-label="Collection" className="input" value={value} onChange={(event) => onChange(event.target.value)}><option value="">Select a collection</option>{collections?.map((item) => <option key={item.name}>{item.name}</option>)}</select>; }

function Explorer({ apiKey }: { apiKey: string }) {
  const collections = useCollections(apiKey);
  const cache = useQueryClient();
  const [name, setName] = useState(""); const [after, setAfter] = useState<string | undefined>(); const [filter, setFilter] = useState("");
  const records = useQuery({ queryKey: ["vectors", name, after, apiKey], enabled: Boolean(name), queryFn: () => apiFetch<VectorRecord[]>(`/v1/collections/${name}/vectors?limit=100${after ? `&after=${encodeURIComponent(after)}` : ""}`, {}, apiKey) });
  const remove = useMutation({ mutationFn: (id: string) => apiFetch<void>(`/v1/collections/${name}/vectors/${id}`, { method: "DELETE" }, apiKey), onSuccess: () => { cache.invalidateQueries({ queryKey: ["vectors", name] }); cache.invalidateQueries({ queryKey: ["collections"] }); } });
  const visible = useMemo(
    () => records.data?.filter((record) => !filter || record.id.includes(filter) || JSON.stringify(record.metadata).toLowerCase().includes(filter.toLowerCase())) ?? [],
    [filter, records.data],
  );
  const points = useMemo(() => projectPca(visible), [visible]);
  return <>
    <Heading title="Vector explorer" description="Browse durable records and inspect a PCA projection of the current page." />
    <ErrorMessage error={records.error ?? remove.error} />
    <div className="panel form-grid"><Field label="Collection"><CollectionSelect collections={collections.data} value={name} onChange={(value) => { setName(value); setAfter(undefined); }} /></Field><Field label="Filter ID or metadata"><input className="input" value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="research" /></Field></div>
    {name && <div className="two-column"><div className="panel"><h3>PCA projection</h3><div className="projection">{points.map((point) => <span aria-label={`Projected vector ${point.id}`} className="projection-point" data-id={point.id} key={point.id} style={{ left: `${point.x}%`, top: `${point.y}%` }} />)}</div><p className="muted">The first two principal components summarize variance in this page; the projection does not preserve every high-dimensional relationship.</p></div><div className="panel"><h3>Records ({visible.length})</h3><div className="table-wrap"><table className="data-table"><thead><tr><th>ID</th><th>Dims</th><th>Metadata</th><th /></tr></thead><tbody>{visible.map((record) => <tr key={record.id}><td><code>{record.id}</code></td><td>{record.values.length}</td><td className="mono">{JSON.stringify(record.metadata)}</td><td><button aria-label={`Delete vector ${record.id}`} className="action-button danger" onClick={() => confirm(`Delete ${record.id}?`) && remove.mutate(record.id)}><Trash2 size={12} /></button></td></tr>)}</tbody></table></div><div className="actions" style={{ marginTop: 12 }}><button className="action-button" disabled={!after} onClick={() => setAfter(undefined)}>First page</button><button className="action-button" disabled={(records.data?.length ?? 0) < 100} onClick={() => setAfter(records.data?.at(-1)?.id)}>Next page</button></div></div></div>}
    {!name && <div className="panel"><Empty icon={VectorSquare} text="Select a collection to inspect its vectors" /></div>}
  </>;
}

function Playground({ apiKey }: { apiKey: string }) {
  const collections = useCollections(apiKey); const [name, setName] = useState(""); const [vector, setVector] = useState("[1, 0]"); const [topK, setTopK] = useState("10"); const [mode, setMode] = useState("approximate"); const [filter, setFilter] = useState("");
  const search = useMutation({ mutationFn: () => { const values = JSON.parse(vector); const expression = filter.trim() ? JSON.parse(filter) : undefined; return apiFetch<SearchResponse>(`/v1/collections/${name}/search`, { method: "POST", body: JSON.stringify({ vector: values, top_k: Number(topK), mode, filter: expression }) }, apiKey); } });
  return <><Heading title="Search playground" description="Execute measured exact or approximate queries against the live engine." /><ErrorMessage error={search.error} /><div className="two-column"><form className="panel form-grid" onSubmit={(event) => { event.preventDefault(); search.mutate(); }}><Field label="Collection" full><CollectionSelect collections={collections.data} value={name} onChange={setName} /></Field><Field label="Query vector" full><textarea className="input textarea" value={vector} onChange={(event) => setVector(event.target.value)} /></Field><Field label="Top K"><input className="input" type="number" min="1" value={topK} onChange={(event) => setTopK(event.target.value)} /></Field><Field label="Mode"><select className="input" value={mode} onChange={(event) => setMode(event.target.value)}><option value="approximate">approximate</option><option value="exact">exact</option></select></Field><Field label="Filter JSON (optional)" full><textarea className="input textarea" value={filter} onChange={(event) => setFilter(event.target.value)} placeholder={'{"eq":{"field":"category","value":"research"}}'} /></Field><button className="button" disabled={!name || search.isPending}>{search.isPending ? "Searching…" : "Run search"}</button></form><div className="panel"><h3>Results {search.data && <span className="muted">· {search.data.took_ms.toFixed(3)} ms</span>}</h3>{search.data ? <div className="table-wrap"><table className="data-table"><thead><tr><th>Rank</th><th>ID</th><th>Score</th><th>Metadata</th></tr></thead><tbody>{search.data.results.map((result, index) => <tr key={result.id}><td>{index + 1}</td><td><code>{result.id}</code></td><td className="mono">{result.score.toFixed(6)}</td><td className="mono">{JSON.stringify(result.metadata)}</td></tr>)}</tbody></table></div> : <Empty icon={Search} text="Run a query to inspect ranked neighbors" />}</div></div></>;
}

function IndexConfiguration({ apiKey }: { apiKey: string }) { const collections = useCollections(apiKey); return <><Heading title="Index configuration" description="Live immutable collection settings. Create a new collection to change construction parameters." /><div className="table-wrap"><table className="data-table"><thead><tr><th>Collection</th><th>M</th><th>ef construction</th><th>ef search</th><th>Seed</th></tr></thead><tbody>{collections.data?.map((item) => <tr key={item.name}><td><code>{item.name}</code></td><td>{item.hnsw.m}</td><td>{item.hnsw.ef_construction}</td><td>{item.hnsw.ef_search}</td><td className="mono">{item.hnsw.random_seed}</td></tr>)}</tbody></table></div></>; }

function Performance({ apiKey }: { apiKey: string }) {
  const metrics = useQuery({ queryKey: ["metrics", apiKey], queryFn: () => fetchMetrics(apiKey), refetchInterval: 5_000 });
  const searches = metrics.data ? metricTotal(metrics.data, "axion_searches_total") : undefined;
  const requests = metrics.data ? metricTotal(metrics.data, "axion_http_requests_total") : undefined;
  const counters = [{ metric: "Searches", value: searches ?? 0 }, { metric: "HTTP responses", value: requests ?? 0 }];
  return <><Heading title="Performance" description="Actual Prometheus process counters refreshed every five seconds; no synthetic data." /><ErrorMessage error={metrics.error} /><div className="metric-grid"><Metric label="Searches" value={searches ?? "—"} note="Process lifetime total" /><Metric label="HTTP responses" value={requests ?? "—"} note="Process lifetime total" /><Metric label="Refresh interval" value="5 s" note="Browser polling" /><Metric label="Latency percentiles" value="Prometheus" note="Derive from histogram buckets" /></div><div className="panel" style={{ height: 380 }}><h3>Current process counters</h3><ResponsiveContainer width="100%" height="88%"><BarChart data={counters}><CartesianGrid stroke="#20242a" vertical={false} /><XAxis dataKey="metric" stroke="#626b75" fontSize={9} /><YAxis stroke="#626b75" fontSize={9} allowDecimals={false} /><Tooltip contentStyle={{ background: "#0b0d10", border: "1px solid #2a2f36", fontSize: 11 }} /><Bar dataKey="value" fill="#6ee7b7" radius={[4, 4, 0, 0]} /></BarChart></ResponsiveContainer></div></>;
}

function Health({ apiKey, health }: { apiKey: string; health?: string }) { const live = useQuery({ queryKey: ["live"], queryFn: () => apiFetch<{ status: string }>("/health/live", {}, apiKey), refetchInterval: 5_000 }); return <><Heading title="System health" description="Direct liveness and storage-backed readiness diagnostics." /><div className="metric-grid"><Metric label="Process" value={live.data?.status === "ok" ? "Alive" : "Unavailable"} note="/health/live" /><Metric label="Storage" value={health === "ready" ? "Ready" : "Unavailable"} note="RocksDB + collection scan" /><Metric label="REST API" value={live.isSuccess ? "Responding" : "Unavailable"} note="Current browser session" /><Metric label="Index status" value={health === "ready" ? "Loaded" : "Unknown"} note="Startup reconstruction gate" /></div><div className="panel"><h3>Runtime</h3><div className="json-result">{JSON.stringify({ api_base: "/api", user_agent: typeof navigator === "undefined" ? "" : navigator.userAgent, checked_at: new Date().toISOString() }, null, 2)}</div></div></>; }

function Documentation() { return <><Heading title="API documentation" description="The live server publishes its versioned OpenAPI contract." /><div className="two-column"><div className="panel"><Code2 size={22} className="success" /><h3 style={{ marginTop: 22 }}>REST / OpenAPI 3.1</h3><p className="muted">Explore every collection, vector, and search operation against the current server.</p><a className="button button-small" href="/api/docs" target="_blank">Open Swagger UI</a></div><div className="panel"><CircleGauge size={22} className="success" /><h3 style={{ marginTop: 22 }}>gRPC / protobuf</h3><p className="muted">The <code className="mono">axion.v1.Axion</code> contract listens on port 50051. Source is under <code className="mono">proto/axion/v1</code>.</p></div></div></>; }

function SettingsPanel({ apiKey, setApiKey }: { apiKey: string; setApiKey: (value: string) => void }) { const [draft, setDraft] = useState(apiKey); const [saved, setSaved] = useState(false); return <><Heading title="Settings" description="Session-local console connection settings." /><form className="panel form-grid" onSubmit={(event) => { event.preventDefault(); sessionStorage.setItem("axion-api-key", draft); setApiKey(draft); setSaved(true); queryClient.invalidateQueries(); }}><Field label="API base"><input className="input" value="/api (Next.js server proxy)" disabled /></Field><Field label="Bearer API key"><input className="input" type="password" value={draft} onChange={(event) => { setDraft(event.target.value); setSaved(false); }} placeholder="Optional" /></Field><div><button className="button button-small">Save for this tab</button>{saved && <span className="success" style={{ marginLeft: 12, fontSize: 11 }}>Saved</span>}</div></form><p className="muted">The key is stored in sessionStorage and is cleared when this browser tab session ends.</p></>; }

