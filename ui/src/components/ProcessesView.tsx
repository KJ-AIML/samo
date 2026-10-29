import React from "react";
import { ProcessIdentity, ProcessRow, formatBytes } from "../types";

interface ProcessesViewProps {
  processes: ProcessRow[];
  totalProcesses: number;
  selfPid: number;
  includeSelf: boolean;
  onToggleIncludeSelf: (val: boolean) => void;
  searchQuery: string;
  onChangeSearch: (q: string) => void;
  sortBy: string;
  onChangeSortBy: (s: string) => void;
  onTerminateProcess: (identity: ProcessIdentity, signal: "SIGTERM" | "SIGKILL") => Promise<void>;
}

export const ProcessesView: React.FC<ProcessesViewProps> = ({
  processes,
  totalProcesses,
  selfPid,
  includeSelf,
  onToggleIncludeSelf,
  searchQuery,
  onChangeSearch,
  sortBy,
  onChangeSortBy,
  onTerminateProcess,
}) => {
  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
      <div className="top-header">
        <div className="top-title">
          <span>▤</span>
          <span>Processes</span>
          <span
            className="mono-font"
            style={{ fontSize: "12px", color: "var(--text-secondary)" }}
          >
            ({totalProcesses} total • Samo PID {selfPid})
          </span>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: "6px",
              fontSize: "12.5px",
              color: "var(--text-secondary)",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={includeSelf}
              onChange={(e) => onToggleIncludeSelf(e.target.checked)}
            />
            Self-monitoring (Include Samo)
          </label>

          <select
            value={sortBy}
            onChange={(e) => onChangeSortBy(e.target.value)}
            className="btn"
            style={{ padding: "5px 10px" }}
          >
            <option value="cpu">Sort: CPU %</option>
            <option value="memory">Sort: Memory</option>
            <option value="disk">Sort: Disk I/O</option>
            <option value="pid">Sort: PID</option>
            <option value="name">Sort: Name</option>
          </select>

          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onChangeSearch(e.target.value)}
            placeholder="Filter process name, path, or PID…"
            style={{
              width: "230px",
              padding: "6px 10px",
              borderRadius: "6px",
              border: "1px solid var(--border-color)",
              background: "var(--bg-panel)",
              color: "var(--text-primary)",
              fontSize: "12.5px",
            }}
          />
        </div>
      </div>

      <div style={{ flex: 1, overflowY: "auto", background: "var(--bg-panel)" }}>
        <table className="table-grid">
          <thead>
            <tr>
              <th>PID • Start Identity</th>
              <th>Process Name</th>
              <th>CPU %</th>
              <th>Memory</th>
              <th>Disk R/W</th>
              <th>Energy</th>
              <th>Identity-Safe Action (R4)</th>
            </tr>
          </thead>
          <tbody>
            {processes.map((proc) => (
              <tr key={`${proc.identity.pid}-${proc.identity.start_time_secs}`}>
                <td className="mono-font">
                  <strong>{proc.identity.pid}</strong>{" "}
                  <span style={{ color: "var(--text-muted)", fontSize: "11px" }}>
                    @{proc.identity.start_time_secs}
                  </span>
                </td>
                <td>
                  <div style={{ fontWeight: 600 }}>
                    {proc.name}{" "}
                    {proc.is_self && (
                      <span
                        style={{
                          fontSize: "10px",
                          padding: "1px 5px",
                          borderRadius: "4px",
                          background: "var(--bg-active-nav)",
                          color: "var(--accent)",
                        }}
                      >
                        SAMO
                      </span>
                    )}
                  </div>
                  <div
                    className="mono-font"
                    style={{
                      fontSize: "11px",
                      color: "var(--text-muted)",
                      maxWidth: "360px",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {proc.executable_path}
                  </div>
                </td>
                <td className="mono-font">{proc.cpu_pct.toFixed(1)}%</td>
                <td className="mono-font">{formatBytes(proc.memory_bytes)}</td>
                <td className="mono-font">
                  {formatBytes(proc.disk_read_bytes_per_sec + proc.disk_write_bytes_per_sec)}
                </td>
                <td>{proc.energy_impact.replace("_", " ")}</td>
                <td>
                  <div style={{ display: "flex", gap: "6px" }}>
                    <button
                      className="btn"
                      style={{ padding: "3px 8px", fontSize: "11px" }}
                      onClick={() => onTerminateProcess(proc.identity, "SIGTERM")}
                    >
                      Quit
                    </button>
                    <button
                      className="btn btn-danger"
                      style={{ padding: "3px 8px", fontSize: "11px" }}
                      onClick={() => onTerminateProcess(proc.identity, "SIGKILL")}
                    >
                      Force Quit
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
