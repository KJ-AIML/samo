import React from "react";
import { Reading, TelemetryPoint, TelemetrySnapshot, formatBytes } from "../types";

interface PerformanceViewProps {
  telemetry: TelemetrySnapshot | null;
}

function renderReading<T>(reading: Reading<T> | undefined, fmt: (val: T) => string): string {
  if (!reading) return "—";
  if (reading.state === "available") return fmt(reading.value);
  if (reading.state === "stopped") return "GAP (Stopped)";
  return `Unsupported (${reading.reason})`;
}

/**
 * R3: Honest polyline chart that breaks the line whenever a sample is `stopped` or `unsupported`,
 * leaving a real visual gap instead of interpolating across missing samples.
 */
const HonestLineChart: React.FC<{
  points: TelemetryPoint[];
  extract: (pt: TelemetryPoint) => Reading<number>;
  maxVal?: number;
}> = ({ points, extract, maxVal = 100 }) => {
  const width = 600;
  const height = 110;
  const maxPoints = 60;
  const slice = points.slice(-maxPoints);

  const segments: string[][] = [[]];
  slice.forEach((pt, idx) => {
    const r = extract(pt);
    const x = (idx / Math.max(1, maxPoints - 1)) * width;
    if (r.state === "available") {
      const clamped = Math.min(maxVal, Math.max(0, r.value));
      const y = height - (clamped / Math.max(1, maxVal)) * (height - 16) - 8;
      segments[segments.length - 1].push(`${x.toFixed(1)},${y.toFixed(1)}`);
    } else if (segments[segments.length - 1].length > 0) {
      segments.push([]);
    }
  });

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      style={{
        width: "100%",
        height: "110px",
        background: "var(--bg-canvas)",
        borderRadius: "6px",
        border: "1px solid var(--border-color)",
      }}
    >
      {segments
        .filter((s) => s.length > 0)
        .map((seg, idx) => (
          <polyline
            key={idx}
            fill="none"
            stroke="var(--accent)"
            strokeWidth="2.2"
            points={seg.join(" ")}
          />
        ))}
    </svg>
  );
};

export const PerformanceView: React.FC<PerformanceViewProps> = ({ telemetry }) => {
  const latest = telemetry?.latest;
  const mem = latest?.memory.state === "available" ? latest.memory.value : null;
  const pCores =
    latest?.cpu_perf_cores_pct.state === "available" ? latest.cpu_perf_cores_pct.value : null;
  const eCores =
    latest?.cpu_eff_cores_pct.state === "available" ? latest.cpu_eff_cores_pct.value : null;

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "20px" }}>
      <h2 className="mono-font" style={{ fontSize: "22px", fontWeight: 600, marginBottom: "16px" }}>
        Live Performance Telemetry
      </h2>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "16px" }}>
        {/* CPU History + Apple Silicon P/E Cores */}
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border-color)",
            borderRadius: "8px",
            padding: "16px",
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "10px" }}>
            <strong>CPU Overall (R3 Honest Gap Chart)</strong>
            <span className="mono-font">
              {renderReading(latest?.cpu_overall_pct, (v) => `${v.toFixed(1)}%`)}
            </span>
          </div>
          <HonestLineChart
            points={telemetry?.history ?? []}
            extract={(pt) => pt.cpu_overall_pct}
            maxVal={100}
          />

          <div style={{ marginTop: "14px" }}>
            <div style={{ fontSize: "12.5px", fontWeight: 600, marginBottom: "6px" }}>
              Performance Cores (P-Cores)
            </div>
            {pCores ? (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(68px, 1fr))", gap: "6px" }}>
                {pCores.map((pct, i) => (
                  <div
                    key={i}
                    style={{
                      background: "var(--bg-elevated)",
                      padding: "6px",
                      borderRadius: "4px",
                      fontSize: "11.5px",
                    }}
                  >
                    <div>P{i + 1}</div>
                    <div className="mono-font" style={{ fontWeight: 700, color: "var(--accent)" }}>
                      {pct.toFixed(0)}%
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                {renderReading(latest?.cpu_perf_cores_pct, () => "")}
              </div>
            )}

            <div style={{ fontSize: "12.5px", fontWeight: 600, margin: "12px 0 6px" }}>
              Efficiency Cores (E-Cores)
            </div>
            {eCores ? (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(68px, 1fr))", gap: "6px" }}>
                {eCores.map((pct, i) => (
                  <div
                    key={i}
                    style={{
                      background: "var(--bg-elevated)",
                      padding: "6px",
                      borderRadius: "4px",
                      fontSize: "11.5px",
                    }}
                  >
                    <div>E{i + 1}</div>
                    <div className="mono-font" style={{ fontWeight: 700, color: "var(--good)" }}>
                      {pct.toFixed(0)}%
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                {renderReading(latest?.cpu_eff_cores_pct, () => "")}
              </div>
            )}
          </div>
        </div>

        {/* Memory Breakdown */}
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border-color)",
            borderRadius: "8px",
            padding: "16px",
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "10px" }}>
            <strong>Memory Breakdown & Pressure</strong>
            <span className="mono-font">
              {mem ? `${formatBytes(mem.used_bytes)} / ${formatBytes(mem.total_bytes)}` : "—"}
            </span>
          </div>

          <div className="kv-row">
            <span>Memory Pressure</span>
            <strong style={{ color: mem?.pressure === "normal" ? "var(--good)" : "var(--warn)" }}>
              {mem?.pressure.toUpperCase() ?? "—"}
            </strong>
          </div>
          <div className="kv-row">
            <span>Available Memory</span>
            <span className="mono-font">{mem ? formatBytes(mem.available_bytes) : "—"}</span>
          </div>
          <div className="kv-row">
            <span>Wired Memory (macOS vm_stat)</span>
            <span className="mono-font">{renderReading(mem?.wired_bytes, formatBytes)}</span>
          </div>
          <div className="kv-row">
            <span>Compressor Occupied</span>
            <span className="mono-font">{renderReading(mem?.compressed_bytes, formatBytes)}</span>
          </div>
          <div className="kv-row">
            <span>File-backed Cache</span>
            <span className="mono-font">{renderReading(mem?.cached_bytes, formatBytes)}</span>
          </div>
          <div className="kv-row">
            <span>Swap Used / Total</span>
            <span className="mono-font">
              {mem
                ? `${formatBytes(mem.swap_used_bytes)} / ${formatBytes(mem.swap_total_bytes)}`
                : "—"}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};
