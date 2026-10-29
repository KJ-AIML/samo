import React, { useState } from "react";
import {
  CATEGORY_COLORS,
  FileNodeIdentity,
  PackedBall,
  ScanHistoryItem,
  ScanSnapshot,
  formatBytes,
} from "../types";
import { BallsCanvas } from "./BallsCanvas";

interface DiskSpaceViewProps {
  snapshot: ScanSnapshot | null;
  history: ScanHistoryItem[];
  scanPathInput: string;
  onChangeScanPath: (val: string) => void;
  isScanning: boolean;
  onTriggerScan: (customPath?: string) => void;
  onSelectHistoryScan: (scanId: string) => void;
  onMoveToTrash: (targetPath: string, identity: FileNodeIdentity) => Promise<void>;
  onRevealInFinder: (path: string) => Promise<void>;
  statusBanner: string | null;
}

export const DiskSpaceView: React.FC<DiskSpaceViewProps> = ({
  snapshot,
  history,
  scanPathInput,
  onChangeScanPath,
  isScanning,
  onTriggerScan,
  onSelectHistoryScan,
  onMoveToTrash,
  onRevealInFinder,
  statusBanner,
}) => {
  const [viewMode, setViewMode] = useState<"balls" | "treemap" | "projects">("balls");
  const [showLargestFiles, setShowLargestFiles] = useState<boolean>(true);
  const [selectedBall, setSelectedBall] = useState<PackedBall | null>(null);
  const [trashingPath, setTrashingPath] = useState<string | null>(null);

  const usedPct =
    snapshot && snapshot.volume_total_bytes > 0
      ? Math.min(
          100,
          (snapshot.volume_used_bytes / snapshot.volume_total_bytes) * 100
        )
      : 60;

  const handleTrashClick = async (path: string, identity: FileNodeIdentity) => {
    setTrashingPath(path);
    try {
      await onMoveToTrash(path, identity);
    } finally {
      setTrashingPath(null);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
      <div className="top-header">
        <div className="top-title">
          <span>◫</span>
          <span>Disk Space</span>
          <span
            style={{
              fontSize: "11px",
              padding: "3px 8px",
              borderRadius: "999px",
              background: "rgba(52, 211, 153, 0.14)",
              color: "var(--good)",
              fontWeight: 600,
            }}
          >
            Unlocked Personal Edition
          </span>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <input
            type="text"
            value={scanPathInput}
            onChange={(e) => onChangeScanPath(e.target.value)}
            placeholder="/Users/yourname/Developer"
            className="mono-font"
            style={{
              width: "310px",
              padding: "6px 10px",
              borderRadius: "6px",
              border: "1px solid var(--border-color)",
              background: "var(--bg-panel)",
              color: "var(--text-primary)",
              fontSize: "12.5px",
            }}
          />
          <button
            className="btn btn-start"
            disabled={isScanning}
            onClick={() => onTriggerScan()}
          >
            {isScanning ? "Scanning…" : "Start"}
          </button>
        </div>
      </div>

      {statusBanner && (
        <div
          style={{
            padding: "8px 18px",
            background: "var(--bg-elevated)",
            borderBottom: "1px solid var(--border-color)",
            fontSize: "12.5px",
            color: "var(--good)",
          }}
        >
          {statusBanner}
        </div>
      )}

      <div className="disk-layout">
        {/* Left Main Area */}
        <div className="disk-left">
          <div className="disk-subbar">
            <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
              <span>
                <strong>{snapshot?.volume_name || "Macintosh HD"}</strong>
              </span>
              <span style={{ color: "var(--text-secondary)" }}>
                Group: <strong>File Size & Category</strong>
              </span>
              <span style={{ display: "inline-flex", gap: "6px", alignItems: "center" }}>
                View:
                <button
                  className="btn"
                  style={{
                    padding: "3px 9px",
                    borderColor: viewMode === "balls" ? "var(--accent)" : undefined,
                  }}
                  onClick={() => setViewMode("balls")}
                >
                  Balls
                </button>
                <button
                  className="btn"
                  style={{
                    padding: "3px 9px",
                    borderColor: viewMode === "treemap" ? "var(--accent)" : undefined,
                  }}
                  onClick={() => setViewMode("treemap")}
                >
                  Treemap
                </button>
                <button
                  className="btn"
                  style={{
                    padding: "3px 9px",
                    borderColor: viewMode === "projects" ? "var(--accent)" : undefined,
                  }}
                  onClick={() => setViewMode("projects")}
                >
                  Dev Projects ({snapshot?.projects.length ?? 0})
                </button>
              </span>
            </div>

            <span className="mono-font" style={{ color: "var(--text-secondary)", fontSize: "12px" }}>
              {snapshot?.root_path || scanPathInput}
            </span>
          </div>

          {viewMode === "balls" && (
            <BallsCanvas
              balls={snapshot?.balls ?? []}
              selectedBallId={selectedBall?.id ?? null}
              onSelectBall={(b) => setSelectedBall(b)}
            />
          )}

          {viewMode === "treemap" && (
            <div style={{ padding: "18px", flex: 1 }}>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fill, minmax(175px, 1fr))",
                  gap: "10px",
                }}
              >
                {(snapshot?.largest_folders ?? []).map((folder) => (
                  <div
                    key={folder.path}
                    style={{
                      background: "var(--bg-panel)",
                      border: `1px solid ${CATEGORY_COLORS[folder.category]}`,
                      borderRadius: "7px",
                      padding: "12px",
                      display: "flex",
                      flexDirection: "column",
                      justifyContent: "space-between",
                      minHeight: "96px",
                    }}
                  >
                    <div>
                      <div style={{ fontWeight: 700, fontSize: "13.5px" }}>{folder.name}</div>
                      <div
                        className="mono-font"
                        style={{
                          fontSize: "11px",
                          color: "var(--text-secondary)",
                          marginTop: "3px",
                          wordBreak: "break-all",
                        }}
                      >
                        {folder.path}
                      </div>
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        marginTop: "10px",
                      }}
                    >
                      <span className="mono-font" style={{ fontWeight: 700 }}>
                        {formatBytes(folder.bytes)}
                      </span>
                      <div style={{ display: "flex", gap: "5px" }}>
                        <button
                          className="btn"
                          style={{ padding: "3px 7px", fontSize: "11px" }}
                          onClick={() => onRevealInFinder(folder.path)}
                        >
                          Finder
                        </button>
                        <button
                          className="btn btn-danger"
                          style={{ padding: "3px 7px", fontSize: "11px" }}
                          disabled={trashingPath === folder.path}
                          onClick={() => handleTrashClick(folder.path, folder.identity)}
                        >
                          Trash
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {viewMode === "projects" && (
            <div style={{ padding: "18px", flex: 1 }}>
              <div style={{ marginBottom: "10px", color: "var(--text-secondary)", fontSize: "13px" }}>
                Outermost developer projects detected under{" "}
                <span className="mono-font">{snapshot?.root_path}</span> with reclaimable build/cache
                artifacts (`target/`, `node_modules/`, `.venv/`, `.next/`, `dist/`):
              </div>
              <table className="table-grid">
                <thead>
                  <tr>
                    <th>Project</th>
                    <th>Type</th>
                    <th>Total Size</th>
                    <th>Source</th>
                    <th>Reclaimable Artifacts</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {(snapshot?.projects ?? []).map((proj) => (
                    <tr key={proj.root_path}>
                      <td>
                        <div style={{ fontWeight: 600 }}>{proj.name}</div>
                        <div
                          className="mono-font"
                          style={{ fontSize: "11px", color: "var(--text-secondary)" }}
                        >
                          {proj.root_path}
                        </div>
                      </td>
                      <td>{proj.kind}</td>
                      <td className="mono-font">{formatBytes(proj.total_bytes)}</td>
                      <td className="mono-font">{formatBytes(proj.source_bytes)}</td>
                      <td className="mono-font" style={{ color: "var(--warn)", fontWeight: 700 }}>
                        {formatBytes(proj.artifact_bytes)}
                      </td>
                      <td>
                        <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }}>
                          <button
                            className="btn"
                            style={{ padding: "3px 8px", fontSize: "11px" }}
                            onClick={() => onRevealInFinder(proj.root_path)}
                          >
                            Finder
                          </button>
                          {proj.artifacts.map((art) => (
                            <button
                              key={art.path}
                              className="btn btn-danger"
                              style={{ padding: "3px 8px", fontSize: "11px" }}
                              disabled={trashingPath === art.path}
                              onClick={() => handleTrashClick(art.path, art.identity)}
                            >
                              Trash {art.name} ({formatBytes(art.bytes)})
                            </button>
                          ))}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {selectedBall && (
            <div
              style={{
                padding: "10px 18px",
                borderTop: "1px solid var(--border-color)",
                background: "var(--bg-panel)",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                fontSize: "13px",
              }}
            >
              <div>
                <strong>{selectedBall.label}</strong>{" "}
                <span className="mono-font" style={{ color: "var(--text-secondary)" }}>
                  ({selectedBall.path})
                </span>{" "}
                — <strong className="mono-font">{formatBytes(selectedBall.bytes)}</strong>
              </div>
              <div style={{ display: "flex", gap: "8px" }}>
                <button
                  className="btn"
                  onClick={() => onTriggerScan(selectedBall.path)}
                >
                  Scan Inside
                </button>
                <button
                  className="btn"
                  onClick={() => onRevealInFinder(selectedBall.path)}
                >
                  Reveal in Finder
                </button>
                <button
                  className="btn btn-danger"
                  disabled={trashingPath === selectedBall.path}
                  onClick={() => handleTrashClick(selectedBall.path, selectedBall.identity)}
                >
                  Move to Trash
                </button>
              </div>
            </div>
          )}

          {/* TMOG green LED progress strip */}
          <div className="scan-led-strip">
            <div style={{ display: "flex", gap: "4px", flex: 1 }}>
              {Array.from({ length: 28 }).map((_, idx) => (
                <div
                  key={idx}
                  className={`scan-led ${snapshot ? "lit" : ""}`}
                />
              ))}
            </div>
            <span style={{ color: "var(--text-secondary)", fontSize: "12px" }}>
              {isScanning
                ? "Scanning filesystem…"
                : snapshot
                ? `Scanned ${formatBytes(snapshot.scanned_bytes)} (${snapshot.scanned_files.toLocaleString()} files)`
                : "Ready"}
            </span>
            <span style={{ fontWeight: 600, fontSize: "12px" }}>
              {snapshot ? "Complete" : "Idle"}
            </span>
          </div>

          {/* Expandable Largest Files section */}
          <div style={{ borderTop: "1px solid var(--border-color)", padding: "10px 18px" }}>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                cursor: "pointer",
              }}
              onClick={() => setShowLargestFiles(!showLargestFiles)}
            >
              <span style={{ fontWeight: 700, fontSize: "14px" }}>
                {showLargestFiles ? "▾" : "▸"} Largest Files
              </span>
              <span style={{ color: "var(--text-secondary)", fontSize: "12px" }}>
                {snapshot?.largest_files.length ?? 0} tracked
              </span>
            </div>

            {showLargestFiles && snapshot && snapshot.largest_files.length > 0 && (
              <div style={{ maxHeight: "200px", overflowY: "auto", marginTop: "8px" }}>
                <table className="table-grid">
                  <thead>
                    <tr>
                      <th>File</th>
                      <th>Category</th>
                      <th>Size</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {snapshot.largest_files.slice(0, 30).map((f) => (
                      <tr key={f.path}>
                        <td>
                          <div style={{ fontWeight: 600 }}>{f.name}</div>
                          <div
                            className="mono-font"
                            style={{ fontSize: "11px", color: "var(--text-secondary)" }}
                          >
                            {f.path}
                          </div>
                        </td>
                        <td>
                          <span
                            className="category-dot"
                            style={{ background: CATEGORY_COLORS[f.category] }}
                          />
                          {f.category}
                        </td>
                        <td className="mono-font" style={{ fontWeight: 600 }}>
                          {formatBytes(f.bytes)}
                        </td>
                        <td>
                          <div style={{ display: "flex", gap: "6px" }}>
                            <button
                              className="btn"
                              style={{ padding: "2px 7px", fontSize: "11px" }}
                              onClick={() => onRevealInFinder(f.path)}
                            >
                              Finder
                            </button>
                            <button
                              className="btn btn-danger"
                              style={{ padding: "2px 7px", fontSize: "11px" }}
                              disabled={trashingPath === f.path}
                              onClick={() => handleTrashClick(f.path, f.identity)}
                            >
                              Trash
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>

        {/* Right Inspector Rail matching TMOG Disk Space screenshot */}
        <div className="disk-right">
          {/* Volume Overview */}
          <div className="inspector-section">
            <div className="inspector-title">
              <span>{snapshot?.volume_name || "Macintosh HD"}</span>
              <span className="mono-font" style={{ color: "var(--text-secondary)", fontSize: "13px" }}>
                {snapshot ? formatBytes(snapshot.volume_total_bytes) : "—"}
              </span>
            </div>
            <div className="usage-bar-track">
              <div className="usage-bar-fill" style={{ width: `${usedPct}%` }} />
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                fontSize: "12px",
                color: "var(--text-secondary)",
              }}
            >
              <span>
                {snapshot ? `${formatBytes(snapshot.volume_used_bytes)} used` : "—"}
              </span>
              <span>
                {snapshot ? `${formatBytes(snapshot.volume_free_bytes)} free` : "—"}
              </span>
            </div>
          </div>

          {/* File Type Breakdown */}
          <div className="inspector-section">
            <div className="inspector-title">
              <span>File Type</span>
              <span style={{ fontSize: "11.5px", color: "var(--text-secondary)", fontWeight: 500 }}>
                {snapshot ? formatBytes(snapshot.scanned_bytes) : ""}
              </span>
            </div>
            {(snapshot?.categories ?? []).map((cat) => (
              <div key={cat.category} className="kv-row">
                <span>
                  <span
                    className="category-dot"
                    style={{ background: CATEGORY_COLORS[cat.category] }}
                  />
                  {cat.label}
                </span>
                <span className="mono-font" style={{ color: "var(--text-secondary)" }}>
                  {formatBytes(cat.bytes)}
                </span>
              </div>
            ))}
          </div>

          {/* Largest Folders */}
          <div className="inspector-section">
            <div className="inspector-title">
              <span>Largest Folders</span>
            </div>
            {(snapshot?.largest_folders ?? []).slice(0, 8).map((folder) => (
              <div
                key={folder.path}
                className="kv-row"
                style={{ cursor: "pointer" }}
                onClick={() => onTriggerScan(folder.path)}
                title={`Click to scan inside ${folder.path}`}
              >
                <span
                  style={{
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    maxWidth: "210px",
                  }}
                >
                  {folder.name}
                </span>
                <span className="mono-font" style={{ color: "var(--text-secondary)" }}>
                  {formatBytes(folder.bytes)}
                </span>
              </div>
            ))}
          </div>

          {/* Dev Projects Quick Reclaim */}
          {snapshot && snapshot.projects.length > 0 && (
            <div className="inspector-section">
              <div className="inspector-title">
                <span>Dev Projects Bloat</span>
                <button
                  className="btn"
                  style={{ padding: "2px 8px", fontSize: "11px" }}
                  onClick={() => setViewMode("projects")}
                >
                  View All
                </button>
              </div>
              {snapshot.projects.slice(0, 5).map((proj) => (
                <div key={proj.root_path} style={{ padding: "5px 0", fontSize: "12.5px" }}>
                  <div style={{ display: "flex", justifyContent: "space-between" }}>
                    <strong>{proj.name}</strong>
                    <span className="mono-font">{formatBytes(proj.total_bytes)}</span>
                  </div>
                  {proj.artifact_bytes > 0 && (
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        marginTop: "3px",
                        color: "var(--warn)",
                        fontSize: "11.5px",
                      }}
                    >
                      <span>Reclaimable: {formatBytes(proj.artifact_bytes)}</span>
                      {proj.artifacts[0] && (
                        <button
                          className="btn btn-danger"
                          style={{ padding: "1px 6px", fontSize: "10.5px" }}
                          disabled={trashingPath === proj.artifacts[0].path}
                          onClick={() =>
                            handleTrashClick(
                              proj.artifacts[0].path,
                              proj.artifacts[0].identity
                            )
                          }
                        >
                          Trash {proj.artifacts[0].name}
                        </button>
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {/* Scan History */}
          <div className="inspector-section" style={{ borderBottom: "none" }}>
            <div className="inspector-title">
              <span>History</span>
              <span className="mono-font" style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                {history.length}
              </span>
            </div>
            {history.slice(0, 6).map((item) => (
              <div
                key={item.scan_id}
                style={{
                  padding: "6px 0",
                  cursor: "pointer",
                  borderBottom: "1px solid rgba(255,255,255,0.04)",
                }}
                onClick={() => onSelectHistoryScan(item.scan_id)}
              >
                <div style={{ display: "flex", justifyContent: "space-between", fontSize: "13px" }}>
                  <strong>{item.root_path.split("/").filter(Boolean).pop() || item.volume_name}</strong>
                  <span className="mono-font" style={{ color: "var(--text-secondary)" }}>
                    {formatBytes(item.scanned_bytes)}
                  </span>
                </div>
                <div style={{ fontSize: "11.5px", color: "var(--text-muted)", marginTop: "2px" }}>
                  {new Date(item.started_at_ms).toLocaleString()}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};
