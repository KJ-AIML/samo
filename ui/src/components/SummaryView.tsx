import React from "react";
import { Reading, ScanSnapshot, TelemetrySnapshot, formatBytes } from "../types";

interface SummaryViewProps {
  telemetry: TelemetrySnapshot | null;
  diskSnapshot: ScanSnapshot | null;
  onNavigateTab: (tab: "disk" | "performance" | "processes") => void;
}

function renderReading<T>(reading: Reading<T> | undefined, fmt: (val: T) => string): string {
  if (!reading) return "—";
  if (reading.state === "available") return fmt(reading.value);
  if (reading.state === "stopped") return "GAP (Stopped)";
  return `Unsupported (${reading.reason})`;
}

export const SummaryView: React.FC<SummaryViewProps> = ({
  telemetry,
  diskSnapshot,
  onNavigateTab,
}) => {
  const latest = telemetry?.latest;
  const mem = latest?.memory.state === "available" ? latest.memory.value : null;
  const diskIo = latest?.disk_io.state === "available" ? latest.disk_io.value : null;
  const net = latest?.network.state === "available" ? latest.network.value : null;

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "20px" }}>
      <div style={{ marginBottom: "18px" }}>
        <h2 className="mono-font" style={{ fontSize: "22px", fontWeight: 600 }}>
          System Summary — {telemetry?.system.hostname ?? "Mac"}
        </h2>
        <div style={{ color: "var(--text-secondary)", fontSize: "13px", marginTop: "4px" }}>
          {telemetry?.system.chip_brand} • {telemetry?.system.os_version} •{" "}
          {telemetry?.system.physical_cores} physical / {telemetry?.system.logical_cores} logical
          cores (P-cores:{" "}
          {renderReading(telemetry?.system.perf_cores, (v) => String(v))}, E-cores:{" "}
          {renderReading(telemetry?.system.eff_cores, (v) => String(v))})
        </div>
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
          gap: "14px",
          marginBottom: "20px",
        }}
      >
        {/* CPU Card */}
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border-color)",
            borderRadius: "8px",
            padding: "16px",
            cursor: "pointer",
          }}
          onClick={() => onNavigateTab("performance")}
        >
          <div style={{ color: "var(--text-secondary)", fontSize: "12px", fontWeight: 600 }}>
            CPU UTILIZATION
          </div>
          <div className="mono-font" style={{ fontSize: "28px", fontWeight: 700, margin: "8px 0" }}>
            {renderReading(latest?.cpu_overall_pct, (v) => `${v.toFixed(1)}%`)}
          </div>
          <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
            Click to inspect P-Core & E-Core telemetry
          </div>
        </div>

        {/* Memory Card */}
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border-color)",
            borderRadius: "8px",
            padding: "16px",
            cursor: "pointer",
          }}
          onClick={() => onNavigateTab("performance")}
        >
          <div style={{ color: "var(--text-secondary)", fontSize: "12px", fontWeight: 600 }}>
            MEMORY ({mem?.pressure.toUpperCase() ?? "NORMAL"})
          </div>
          <div className="mono-font" style={{ fontSize: "28px", fontWeight: 700, margin: "8px 0" }}>
            {mem ? `${formatBytes(mem.used_bytes)} / ${formatBytes(mem.total_bytes)}` : "—"}
          </div>
          <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
            Wired: {renderReading(mem?.wired_bytes, formatBytes)} • Compressed:{" "}
            {renderReading(mem?.compressed_bytes, formatBytes)}
          </div>
        </div>

        {/* Storage Card */}
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border-color)",
            borderRadius: "8px",
            padding: "16px",
            cursor: "pointer",
          }}
          onClick={() => onNavigateTab("disk")}
        >
          <div style={{ color: "var(--text-secondary)", fontSize: "12px", fontWeight: 600 }}>
            DISK SPACE & I/O
          </div>
          <div className="mono-font" style={{ fontSize: "24px", fontWeight: 700, margin: "8px 0" }}>
            {diskSnapshot
              ? `${formatBytes(diskSnapshot.scanned_bytes)} scanned`
              : "Open Disk Space"}
          </div>
          <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
            Live R: {diskIo ? formatBytes(diskIo.read_bytes_per_sec) : "0 B"}/s • W:{" "}
            {diskIo ? formatBytes(diskIo.write_bytes_per_sec) : "0 B"}/s
          </div>
        </div>

        {/* Network Card */}
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border-color)",
            borderRadius: "8px",
            padding: "16px",
          }}
        >
          <div style={{ color: "var(--text-secondary)", fontSize: "12px", fontWeight: 600 }}>
            NETWORK ({net?.primary_interface ?? "en0"})
          </div>
          <div className="mono-font" style={{ fontSize: "22px", fontWeight: 700, margin: "8px 0" }}>
            ↓ {net ? formatBytes(net.rx_bytes_per_sec) : "0 B"}/s • ↑{" "}
            {net ? formatBytes(net.tx_bytes_per_sec) : "0 B"}/s
          </div>
          <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
            Local IP: {net?.local_ip ?? "127.0.0.1"}
          </div>
        </div>
      </div>

      {/* Top Active Processes */}
      <div
        style={{
          background: "var(--bg-panel)",
          border: "1px solid var(--border-color)",
          borderRadius: "8px",
          padding: "16px",
        }}
      >
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: "10px",
          }}
        >
          <span style={{ fontWeight: 700, fontSize: "15px" }}>
            Top Resource Consumers (Identity-Verified)
          </span>
          <button className="btn" onClick={() => onNavigateTab("processes")}>
            Open Processes ({telemetry?.total_processes ?? 0})
          </button>
        </div>

        <table className="table-grid">
          <thead>
            <tr>
              <th>PID (Start Time)</th>
              <th>Process</th>
              <th>CPU %</th>
              <th>Memory</th>
              <th>Energy Impact</th>
            </tr>
          </thead>
          <tbody>
            {(telemetry?.processes ?? []).slice(0, 8).map((p) => (
              <tr key={`${p.identity.pid}-${p.identity.start_time_secs}`}>
                <td className="mono-font">
                  {p.identity.pid}{" "}
                  <span style={{ color: "var(--text-muted)", fontSize: "11px" }}>
                    @{p.identity.start_time_secs}
                  </span>
                </td>
                <td style={{ fontWeight: 600 }}>{p.name}</td>
                <td className="mono-font">{p.cpu_pct.toFixed(1)}%</td>
                <td className="mono-font">{formatBytes(p.memory_bytes)}</td>
                <td>{p.energy_impact.replace("_", " ")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
