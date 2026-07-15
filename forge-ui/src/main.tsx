import React, { useCallback, useEffect, useMemo, useState } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import type { DoctorReport, MetricSnapshot, ProcessMetrics, ServiceStatus } from "./types";
import "./styles.css";

const HISTORY_POINTS = 60;

function formatBytes(bytes: number): string {
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(0)} KiB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`;
}

function Sparkline({ values, label }: { values: number[]; label: string }) {
  const points = useMemo(() => {
    if (values.length < 2) return "";
    const maximum = Math.max(100, ...values);
    return values
      .map((value, index) => {
        const x = (index / (values.length - 1)) * 100;
        const y = 36 - (value / maximum) * 34;
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      })
      .join(" ");
  }, [values]);
  return (
    <svg className="sparkline" viewBox="0 0 100 38" role="img" aria-label={label} preserveAspectRatio="none">
      <defs>
        <linearGradient id={`fill-${label.replaceAll(" ", "-")}`} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="#59e1c0" stopOpacity=".3" />
          <stop offset="100%" stopColor="#59e1c0" stopOpacity="0" />
        </linearGradient>
      </defs>
      {points ? <polyline points={points} fill="none" stroke="#59e1c0" strokeWidth="1.6" vectorEffect="non-scaling-stroke" /> : null}
    </svg>
  );
}

function MetricCard({ label, value, detail, history }: { label: string; value: string; detail: string; history?: number[] }) {
  return (
    <article className="metric-card">
      <div className="metric-heading"><span>{label}</span><span className="live-dot" aria-label="Live" /></div>
      <strong>{value}</strong>
      <small>{detail}</small>
      {history ? <Sparkline values={history} label={`${label} recent history`} /> : null}
    </article>
  );
}

function ProcessTable({ processes }: { processes: ProcessMetrics[] }) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(
    () => processes.filter((process) => process.executable_name.toLowerCase().includes(query.toLowerCase())).slice(0, 30),
    [processes, query],
  );
  return (
    <section className="panel process-panel" aria-labelledby="processes-heading">
      <div className="panel-heading">
        <div><p className="eyebrow">Attribution</p><h2 id="processes-heading">Active processes</h2></div>
        <label className="search"><span className="sr-only">Search processes</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Filter processes" /></label>
      </div>
      <div className="table-scroll">
        <table>
          <thead><tr><th scope="col">Process</th><th scope="col">PID</th><th scope="col">CPU</th><th scope="col">Working set</th><th scope="col">Handles</th></tr></thead>
          <tbody>
            {filtered.map((process) => (
              <tr key={`${process.identity.pid}-${process.identity.creation_time_100ns}`} title={process.executable_path ?? undefined}>
                <td><span className="process-icon">{process.executable_name.slice(0, 1).toUpperCase()}</span>{process.executable_name}</td>
                <td className="mono">{process.identity.pid}</td>
                <td className="mono">{process.cpu_percent == null ? "warming up" : `${process.cpu_percent.toFixed(1)}%`}</td>
                <td className="mono">{process.working_set_bytes == null ? "Unavailable" : formatBytes(process.working_set_bytes)}</td>
                <td className="mono">{process.handle_count ?? "—"}</td>
              </tr>
            ))}
            {!filtered.length ? <tr><td colSpan={5} className="empty">No matching process in the latest real sample.</td></tr> : null}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function App() {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [snapshot, setSnapshot] = useState<MetricSnapshot | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const [memoryHistory, setMemoryHistory] = useState<number[]>([]);
  const [sidebarOpen, setSidebarOpen] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const [nextStatus, nextSnapshot] = await Promise.all([
        invoke<ServiceStatus>("service_status"),
        invoke<MetricSnapshot>("metric_snapshot"),
      ]);
      setStatus(nextStatus);
      setSnapshot(nextSnapshot);
      setCpuHistory((values) => [...values, nextSnapshot.cpu.total_percent ?? 0].slice(-HISTORY_POINTS));
      setMemoryHistory((values) => [...values, nextSnapshot.memory.memory_load_percent].slice(-HISTORY_POINTS));
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    const runDoctor = () => void invoke<DoctorReport>("doctor_report").then(setDoctor).catch(() => setDoctor(null));
    runDoctor();
    const timer = window.setInterval(runDoctor, 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const usedMemory = snapshot ? snapshot.memory.total_physical_bytes - snapshot.memory.available_physical_bytes : 0;
  const collectorHealth = doctor?.checks.find((check) => check.name === "collector");
  const unavailableCapabilities = snapshot
    ? Object.entries(snapshot.capabilities).filter(([, availability]) => availability.state !== "available")
    : [];

  return (
    <div className={sidebarOpen ? "app-shell" : "app-shell sidebar-collapsed"}>
      <aside className="sidebar">
        <div className="brand"><div className="brand-mark"><i /><i /><i /></div><div><strong>ForgePulse</strong><span>Performance laboratory</span></div></div>
        <nav aria-label="Primary">
          <p>Workspace</p>
          <a className="active" href="#dashboard"><span>◫</span>Dashboard</a>
          <a href="#processes"><span>⌘</span>Processes</a>
          <a href="#health"><span>◇</span>System health</a>
          <p>Laboratories</p>
          <a className="disabled" aria-disabled="true"><span>⌁</span>Timeline <small>Next</small></a>
          <a className="disabled" aria-disabled="true"><span>△</span>Incidents <small>Next</small></a>
          <a className="disabled" aria-disabled="true"><span>⚗</span>Experiments <small>Phase 4</small></a>
          <a className="disabled" aria-disabled="true"><span>◎</span>Gaming lab <small>Phase 5</small></a>
        </nav>
        <div className="privacy-badge"><span>Local only</span><p>No telemetry or cloud connection</p></div>
      </aside>

      <main>
        <header>
          <button className="icon-button" onClick={() => setSidebarOpen((open) => !open)} aria-label="Toggle sidebar">☰</button>
          <div><p className="eyebrow">Live evidence</p><h1>System overview</h1></div>
          <div className="header-status"><span className={error ? "status-dot error" : "status-dot"} />{error ? "Service offline" : "Monitoring locally"}</div>
        </header>

        {error ? (
          <section className="error-state" role="alert">
            <div>!</div><h2>Waiting for the ForgePulse service</h2>
            <p>{error}</p><code>cargo run -p forge-service -- console</code>
            <button onClick={() => void refresh()}>Retry connection</button>
          </section>
        ) : loading || !snapshot || !status ? (
          <section className="loading-state" aria-live="polite"><div className="spinner" /><p>Connecting to the local collector…</p></section>
        ) : (
          <>
            <section className="metric-grid" id="dashboard">
              <MetricCard label="CPU" value={snapshot.cpu.total_percent == null ? "Warming up" : `${snapshot.cpu.total_percent.toFixed(1)}%`} detail={`${snapshot.cpu.logical_processor_count} logical processors`} history={cpuHistory} />
              <MetricCard label="Memory" value={`${snapshot.memory.memory_load_percent}%`} detail={`${formatBytes(usedMemory)} of ${formatBytes(snapshot.memory.total_physical_bytes)}`} history={memoryHistory} />
              <MetricCard label="Processes" value={snapshot.processes.length.toLocaleString()} detail="queryable in latest sample" />
              <MetricCard label="Collector cost" value={`${(snapshot.collection_duration_us / 1000).toFixed(1)} ms`} detail={`every ${status.sampling_interval_ms} ms · ${status.dropped_samples} dropped`} />
            </section>

            <section className="evidence-grid" id="health">
              <article className="panel system-card">
                <div className="panel-heading"><div><p className="eyebrow">Evidence quality</p><h2>Collector status</h2></div><span className={`health-pill ${doctor?.healthy ? "healthy" : "warning"}`}>{doctor?.healthy ? "Healthy" : "Review"}</span></div>
                <dl>
                  <div><dt>Latest sample</dt><dd>{new Date(snapshot.captured_at).toLocaleTimeString()}</dd></div>
                  <div><dt>Samples retained</dt><dd>{status.samples_collected.toLocaleString()}</dd></div>
                  <div><dt>Database size</dt><dd>{formatBytes(status.database_bytes)}</dd></div>
                  <div><dt>Collector</dt><dd>{collectorHealth?.details ?? "Checking"}</dd></div>
                </dl>
              </article>
              <article className="panel capability-card">
                <div className="panel-heading"><div><p className="eyebrow">Graceful degradation</p><h2>Capabilities</h2></div><span>{Object.keys(snapshot.capabilities).length} checked</span></div>
                {unavailableCapabilities.length ? (
                  <ul>{unavailableCapabilities.map(([name, availability]) => <li key={name}><strong>{name.replaceAll("_", " ")}</strong><span>{availability.state.replaceAll("_", " ")}{"reason" in availability ? ` · ${availability.reason}` : ""}</span></li>)}</ul>
                ) : <div className="empty compact">All foundation collectors are available.</div>}
              </article>
            </section>
            <div id="processes"><ProcessTable processes={snapshot.processes} /></div>
          </>
        )}
      </main>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
