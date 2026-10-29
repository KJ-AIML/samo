import React, { useCallback, useEffect, useState } from "react";
import { BallsCanvas } from "./components/BallsCanvas";
import {
  CATEGORY_COLORS,
  FileNodeIdentity,
  PackedBall,
  ProcessIdentity,
  ScanSnapshot,
  TelemetrySnapshot,
  formatBytes,
} from "./types";

type PanelMode = "status" | "balls" | "projects";

function splitNumberAndUnit(bytes: number): { num: string; unit: string } {
  const formatted = formatBytes(bytes);
  const parts = formatted.split(" ");
  return { num: parts[0] || "0", unit: parts[1] || "B" };
}

export const App: React.FC = () => {
  const [mode, setMode] = useState<PanelMode>("status");
  const [telemetry, setTelemetry] = useState<TelemetrySnapshot | null>(null);
  const [diskSnapshot, setDiskSnapshot] = useState<ScanSnapshot | null>(null);
  const [scanRoot, setScanRoot] = useState<string>("");
  const [isScanning, setIsScanning] = useState<boolean>(false);
  const [toast, setToast] = useState<string | null>(null);
  const [selectedBall, setSelectedBall] = useState<PackedBall | null>(null);

  const triggerScan = useCallback(
    async (customRoot?: string) => {
      const target = (customRoot ?? scanRoot).trim();
      setIsScanning(true);
      try {
        const res = await fetch("/api/disk/scan", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ root_path: target || undefined }),
        });
        const data = await res.json();
        if (res.ok) {
          setDiskSnapshot(data);
          if (data.root_path) setScanRoot(data.root_path);
          setToast(null);
        } else {
          setToast(data?.error?.message || "Scan failed");
        }
      } catch {
        setToast("Connection error");
      } finally {
        setIsScanning(false);
      }
    },
    [scanRoot]
  );

  useEffect(() => {
    (async () => {
      try {
        const [healthRes, latestRes] = await Promise.all([
          fetch("/api/health"),
          fetch("/api/disk/latest"),
        ]);
        let defaultRoot = "/";
        if (healthRes.ok) {
          const h = await healthRes.json();
          if (h.default_scan_root) {
            defaultRoot = h.default_scan_root;
            setScanRoot(h.default_scan_root);
          }
        }
        if (latestRes.ok) {
          const latest = await latestRes.json();
          if (latest) {
            setDiskSnapshot(latest);
            if (latest.root_path) setScanRoot(latest.root_path);
            return;
          }
        }
        await triggerScan(defaultRoot);
      } catch {
        // ignore
      }
    })();
  }, []);

  useEffect(() => {
    let active = true;
    const poll = async () => {
      try {
        const res = await fetch("/api/telemetry/live?sample=false");
        if (res.ok && active) {
          const snap: TelemetrySnapshot = await res.json();
          setTelemetry(snap);
        }
      } catch {
        // ignore
      }
    };
    poll();
    const timer = setInterval(poll, 1000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);

  const handleTrash = async (path: string, identity: FileNodeIdentity) => {
    if (!diskSnapshot) return;
    const res = await fetch("/api/disk/trash", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        scan_id: diskSnapshot.scan_id,
        target_path: path,
        identity,
      }),
    });
    const data = await res.json();
    if (res.ok) {
      setToast(`Released ${formatBytes(data.reclaimed_bytes)} to Trash`);
      await triggerScan(diskSnapshot.root_path);
    } else {
      setToast(data?.error?.message || "Trash guard blocked");
    }
  };

  const handleReleaseTopBloat = async () => {
    const firstProjWithArt = (diskSnapshot?.projects ?? []).find(
      (p) => p.artifacts.length > 0
    );
    if (firstProjWithArt && firstProjWithArt.artifacts[0]) {
      await handleTrash(
        firstProjWithArt.artifacts[0].path,
        firstProjWithArt.artifacts[0].identity
      );
    } else {
      setMode("projects");
    }
  };

  const handleQuitProcess = async (identity: ProcessIdentity) => {
    const res = await fetch("/api/process/terminate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identity, signal: "SIGTERM" }),
    });
    if (res.ok) {
      setToast(`Released PID ${identity.pid}`);
    }
  };

  const volTotal = diskSnapshot?.volume_total_bytes ?? 512_000_000_000;
  const volUsed = diskSnapshot?.volume_used_bytes ?? 380_000_000_000;
  const volFree = diskSnapshot?.volume_free_bytes ?? 126_800_000_000;
  const scannedBytes = diskSnapshot?.scanned_bytes ?? 0;
  const reclaimableBytes = (diskSnapshot?.projects ?? []).reduce(
    (acc, p) => acc + p.artifact_bytes,
    0
  );

  const freeSplit = splitNumberAndUnit(volFree);
  const usedPct = volTotal > 0 ? Math.min(82, (volUsed / volTotal) * 100) : 58;
  const othersPct = Math.min(18, 100 - usedPct);

  const cleanRatioPct =
    scannedBytes > 0
      ? Math.max(
          12,
          Math.min(100, ((scannedBytes - reclaimableBytes) / scannedBytes) * 100)
        )
      : 88;

  const memVal =
    telemetry?.latest.memory.state === "available"
      ? telemetry.latest.memory.value
      : null;
  const memAvailSplit = splitNumberAndUnit(memVal?.available_bytes ?? 6_700_000_000);
  const memPressurePct =
    memVal && memVal.total_bytes > 0
      ? Math.round((memVal.used_bytes / memVal.total_bytes) * 100)
      : 68;
  const litPressureDots = Math.max(1, Math.min(10, Math.round(memPressurePct / 10)));
  const swapUsedPct =
    memVal && memVal.swap_total_bytes > 0
      ? Math.min(100, (memVal.swap_used_bytes / memVal.swap_total_bytes) * 100)
      : 45;

  const cpuPct =
    telemetry?.latest.cpu_overall_pct.state === "available"
      ? telemetry.latest.cpu_overall_pct.value
      : 18.42;

  const netVal =
    telemetry?.latest.network.state === "available"
      ? telemetry.latest.network.value
      : null;
  const txMbps = netVal
    ? ((netVal.tx_bytes_per_sec * 8) / 1_000_000).toFixed(2)
    : "15.67";
  const rxMbps = netVal
    ? ((netVal.rx_bytes_per_sec * 8) / 1_000_000).toFixed(2)
    : "13.71";

  const historyPoints = telemetry?.history ?? [];

  return (
    <div className="utility-window">
      {/* 1. Top Profile & Scope Bar */}
      <header className="top-profile-bar">
        <div className="profile-left">
          <div className="profile-avatar" aria-hidden="true">
            <div className="profile-avatar-dot" />
          </div>
          <div>
            <div className="profile-name">
              <span>KJ · Samo</span>
              <span style={{ color: "var(--text-muted)", fontSize: "12px" }}>
                ›
              </span>
            </div>
            <div className="profile-sub">
              {toast || scanRoot || "Personal Mac Storage & Telemetry"}
            </div>
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <button
            type="button"
            className="icon-btn"
            aria-label="Scan Developer folder"
            title="Scan Developer folder"
            onClick={() => triggerScan()}
          >
            🔍
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label="Switch view"
            title="Switch view"
            onClick={() =>
              setMode(mode === "status" ? "projects" : "status")
            }
          >
            ⋮
          </button>
        </div>
      </header>

      {/* 2. Hero Device Section with 3 Circular Mode Buttons */}
      <section className="hero-device-stage" aria-label="Device mode selector">
        <div className="circle-modes-row">
          {/* Left Circle: Storage Balls Map */}
          <button
            type="button"
            className="circle-mode-btn"
            aria-pressed={mode === "balls"}
            aria-label="Storage Balls Map"
            onClick={() => setMode("balls")}
          >
            <span className="badge-blue" />
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
              <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
            </svg>
          </button>

          {/* Center Circle: Main System Status */}
          <button
            type="button"
            className="circle-mode-btn center-main"
            aria-pressed={mode === "status"}
            aria-label="Main System Status"
            onClick={() => setMode("status")}
          >
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
              <rect x="3" y="4" width="18" height="12" rx="2" />
              <path d="M2 20h20" />
            </svg>
          </button>

          {/* Right Circle: Dev Projects & Bloat Cleanup */}
          <button
            type="button"
            className="circle-mode-btn"
            aria-pressed={mode === "projects"}
            aria-label="Developer Projects & Bloat"
            onClick={() => setMode("projects")}
          >
            <span className="badge-red" />
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" />
            </svg>
          </button>
        </div>

        <h1 className="device-title">
          {telemetry?.system.hostname || "MacBook Pro"} (
          {telemetry?.system.chip_brand || "Apple Silicon"})
        </h1>
        <div className="device-subtitle">
          {telemetry?.system.os_version || "macOS"} •{" "}
          {diskSnapshot
            ? `${formatBytes(diskSnapshot.scanned_bytes)} indexed`
            : "Physical APFS Telemetry"}
        </div>
      </section>

      {/* 3. Folder-Tab STATUS Recessed Tray */}
      <main className="status-folder-wrapper">
        <div className="status-folder-tab">
          {mode === "status"
            ? "STATUS"
            : mode === "balls"
            ? "STORAGE BALLS"
            : "DEV PROJECTS"}
        </div>

        <div className="status-tray">
          {mode === "status" && (
            <>
              {/* ROW 1: Storage + Workspace Health */}
              <div className="row-top-2col">
                {/* Card 1: Available Storage */}
                <div className="slate-card">
                  <div>
                    <div className="metric-head-row">
                      <div>
                        <span className="metric-big-num">{freeSplit.num}</span>
                        <span className="metric-unit">{freeSplit.unit}</span>
                      </div>
                      <button
                        type="button"
                        className="release-btn"
                        onClick={handleReleaseTopBloat}
                      >
                        Release
                      </button>
                    </div>
                    <div className="metric-caption">Available Storage</div>

                    <div className="bar-track" style={{ overflow: "hidden" }}>
                      <div
                        style={{
                          width: `${usedPct}%`,
                          height: "100%",
                          background: "var(--blue-bar)",
                        }}
                      />
                      <div
                        style={{
                          width: `${othersPct}%`,
                          height: "100%",
                          background: "#4c515c",
                        }}
                      />
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        fontSize: "11.5px",
                        color: "var(--text-muted)",
                        marginTop: "6px",
                      }}
                    >
                      <span>Used</span>
                      <span>Others</span>
                    </div>
                  </div>

                  <div className="card-bottom-row">
                    <span
                      style={{ cursor: "pointer" }}
                      onClick={() => setMode("balls")}
                    >
                      {diskSnapshot?.volume_name || "Macintosh HD"} ⌄
                    </span>
                    <span className="tabular-nums">{formatBytes(volTotal)}</span>
                  </div>
                </div>

                {/* Card 2: Clean Workspace Ratio / Reclaimable Bloat */}
                <div className="slate-card">
                  <div>
                    <div className="metric-head-row">
                      <div>
                        <span className="metric-big-num">
                          {cleanRatioPct.toFixed(0)}
                        </span>
                        <span className="metric-unit">%</span>
                      </div>
                    </div>
                    <div className="metric-caption">Clean Workspace Ratio</div>

                    <div className="bar-track">
                      <div
                        style={{
                          width: `${cleanRatioPct}%`,
                          height: "100%",
                          background: "var(--green-bar)",
                          borderRadius: "999px",
                        }}
                      />
                      <div className="green-bolt-badge">⚡</div>
                    </div>
                    <div
                      style={{
                        fontSize: "11.5px",
                        color: "var(--text-muted)",
                        marginTop: "6px",
                      }}
                    >
                      {formatBytes(reclaimableBytes)} reclaimable in{" "}
                      {diskSnapshot?.projects.length ?? 0} projects
                    </div>
                  </div>

                  <div className="card-bottom-row">
                    <span
                      style={{ cursor: "pointer" }}
                      onClick={() => setMode("projects")}
                    >
                      High Performance ⌄
                    </span>
                  </div>
                </div>
              </div>

              {/* ROW 2 + 3: Left Stack (Memory + CPU) + Right Tall Speedometer Card */}
              <div className="row-bottom-asymmetric">
                <div className="left-stack-col">
                  {/* Card 3: Available Memory */}
                  <div className="slate-card">
                    <div className="metric-head-row">
                      <div>
                        <span className="metric-big-num">
                          {memAvailSplit.num}
                        </span>
                        <span className="metric-unit">{memAvailSplit.unit}</span>
                      </div>
                      <button
                        type="button"
                        className="release-btn"
                        onClick={() => setMode("balls")}
                      >
                        Release
                      </button>
                    </div>
                    <div className="metric-caption">Available Memory</div>

                    {/* Dual Gauge Row: 10 Orange LED Dots + Yellow Swap Bar */}
                    <div
                      style={{
                        display: "grid",
                        gridTemplateColumns: "1fr 1fr",
                        gap: "16px",
                        marginTop: "12px",
                        alignItems: "center",
                      }}
                    >
                      <div>
                        <div style={{ display: "flex", gap: "5px" }}>
                          {Array.from({ length: 10 }).map((_, idx) => (
                            <span
                              key={idx}
                              style={{
                                width: "7.5px",
                                height: "7.5px",
                                borderRadius: "50%",
                                background:
                                  idx < litPressureDots
                                    ? "var(--orange-led)"
                                    : "var(--bg-track)",
                              }}
                            />
                          ))}
                        </div>
                        <div
                          style={{
                            display: "flex",
                            justifyContent: "space-between",
                            fontSize: "11.5px",
                            marginTop: "6px",
                          }}
                        >
                          <span style={{ color: "var(--text-muted)" }}>
                            Pressure
                          </span>
                          <span className="tabular-nums">{memPressurePct}%</span>
                        </div>
                      </div>

                      <div>
                        <div
                          style={{
                            width: "100%",
                            height: "6.5px",
                            background: "var(--bg-track)",
                            borderRadius: "999px",
                            overflow: "hidden",
                          }}
                        >
                          <div
                            style={{
                              width: `${swapUsedPct}%`,
                              height: "100%",
                              background: "var(--yellow-bar)",
                            }}
                          />
                        </div>
                        <div
                          style={{
                            display: "flex",
                            justifyContent: "space-between",
                            fontSize: "11.5px",
                            marginTop: "6px",
                          }}
                        >
                          <span style={{ color: "var(--text-muted)" }}>
                            Swap File
                          </span>
                          <span className="tabular-nums">
                            {memVal
                              ? `${formatBytes(memVal.swap_used_bytes)}/${formatBytes(memVal.swap_total_bytes)}`
                              : "0/0 GB"}
                          </span>
                        </div>
                      </div>
                    </div>

                    {/* 3 Top Folder / Project Chips */}
                    <div className="chip-3col">
                      {(diskSnapshot?.largest_folders ?? [])
                        .slice(0, 3)
                        .map((folder) => (
                          <button
                            key={folder.path}
                            type="button"
                            className="app-chip"
                            onClick={() => triggerScan(folder.path)}
                          >
                            <span
                              className="app-chip-icon"
                              style={{
                                background: CATEGORY_COLORS[folder.category],
                              }}
                            >
                              {folder.name.slice(0, 1).toUpperCase()}
                            </span>
                            <span className="app-chip-name">{folder.name}</span>
                          </button>
                        ))}
                    </div>
                  </div>

                  {/* Card 4: CPU Load + 28x6 Cyan LED Dot-Matrix Histogram */}
                  <div className="slate-card">
                    <div className="metric-head-row">
                      <div>
                        <div>
                          <span className="metric-big-num">
                            {cpuPct.toFixed(2)}
                          </span>
                          <span className="metric-unit">%</span>
                        </div>
                        <div className="metric-caption">CPU Load</div>
                      </div>

                      <div
                        style={{
                          background: "var(--bg-card-recessed)",
                          borderRadius: "6px",
                          padding: "6px 10px",
                          fontSize: "11px",
                          display: "grid",
                          gridTemplateColumns: "auto auto auto auto",
                          columnGap: "8px",
                          rowGap: "3px",
                        }}
                      >
                        <span style={{ color: "var(--text-muted)" }}>Cores</span>
                        <span className="tabular-nums">
                          {telemetry?.system.logical_cores ?? 8}
                        </span>
                        <span style={{ color: "var(--text-muted)" }}>Procs</span>
                        <span className="tabular-nums">
                          {telemetry?.total_processes ?? 0}
                        </span>
                        <span style={{ color: "var(--text-muted)" }}>
                          Projects
                        </span>
                        <span
                          className="tabular-nums"
                          style={{ gridColumn: "span 3", textAlign: "end" }}
                        >
                          {diskSnapshot?.projects.length ?? 0} indexed
                        </span>
                      </div>
                    </div>

                    {/* 28x6 Cyan LED Dot-Matrix Histogram */}
                    <svg
                      viewBox="0 0 280 54"
                      style={{
                        width: "100%",
                        height: "54px",
                        marginTop: "12px",
                      }}
                    >
                      {Array.from({ length: 28 }).map((_, col) => {
                        const sampleIdx =
                          historyPoints.length > 28 - col
                            ? historyPoints.length - (28 - col)
                            : 0;
                        const pt = historyPoints[sampleIdx];
                        const val =
                          pt?.cpu_overall_pct.state === "available"
                            ? pt.cpu_overall_pct.value
                            : 18;
                        const litRows = Math.max(
                          1,
                          Math.min(6, Math.ceil((val / 100) * 6))
                        );

                        return Array.from({ length: 6 }).map((__, row) => {
                          const fromBottom = 5 - row;
                          const isLit = fromBottom < litRows;
                          return (
                            <circle
                              key={`${col}-${row}`}
                              cx={col * 10 + 5}
                              cy={row * 9 + 4.5}
                              r="2.4"
                              fill={
                                isLit ? "var(--cyan-led)" : "var(--cyan-led-off)"
                              }
                            />
                          );
                        });
                      })}
                    </svg>

                    {/* 3 Top CPU Process Chips */}
                    <div className="chip-3col">
                      {(telemetry?.processes ?? []).slice(0, 3).map((proc, i) => (
                        <button
                          key={`${proc.identity.pid}-${proc.identity.start_time_secs}`}
                          type="button"
                          className="app-chip"
                          onClick={() => handleQuitProcess(proc.identity)}
                          title={`Click to quit ${proc.name} (PID ${proc.identity.pid})`}
                        >
                          <span
                            className="app-chip-icon"
                            style={{
                              background:
                                i === 0
                                  ? "#1686f7"
                                  : i === 1
                                  ? "#64748b"
                                  : "#0ea5e9",
                            }}
                          >
                            {proc.name.slice(0, 1).toUpperCase()}
                          </span>
                          <span className="app-chip-name">{proc.name}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                {/* Right Tall Card: Radial 44-Tick Speedometer Gauge */}
                <div
                  className="slate-card"
                  style={{ alignItems: "center", textAlign: "center" }}
                >
                  <svg
                    viewBox="0 0 150 150"
                    style={{ width: "142px", height: "142px" }}
                  >
                    <circle
                      cx="75"
                      cy="75"
                      r="66"
                      fill="#1e2025"
                      stroke="#25282f"
                      strokeWidth="1.5"
                    />
                    {Array.from({ length: 44 }).map((_, i) => {
                      const frac = i / 44;
                      const angle = frac * Math.PI * 2 - Math.PI / 2;
                      const x1 = 75 + 48 * Math.cos(angle);
                      const y1 = 75 + 48 * Math.sin(angle);
                      const x2 = 75 + 60 * Math.cos(angle);
                      const y2 = 75 + 60 * Math.sin(angle);
                      const stroke =
                        frac >= 0.55 && frac <= 0.85
                          ? "#2b72ff"
                          : frac >= 0.08 && frac <= 0.42
                          ? "#9b4dff"
                          : "#2d3038";
                      return (
                        <line
                          key={i}
                          x1={x1}
                          y1={y1}
                          x2={x2}
                          y2={y2}
                          stroke={stroke}
                          strokeWidth="2.4"
                          strokeLinecap="round"
                        />
                      );
                    })}
                    {/* Up / Down Arrows */}
                    <text
                      x="75"
                      y="50"
                      textAnchor="middle"
                      fill="#7b808a"
                      fontSize="11"
                    >
                      ↑
                    </text>
                    <text
                      x="75"
                      y="108"
                      textAnchor="middle"
                      fill="#7b808a"
                      fontSize="11"
                    >
                      ↓
                    </text>
                    {/* Tilted Compass Needle */}
                    <line
                      x1="62"
                      y1="96"
                      x2="89"
                      y2="51"
                      stroke="#f5f7fa"
                      strokeWidth="2.8"
                      strokeLinecap="round"
                    />
                    <circle
                      cx="75"
                      cy="75"
                      r="5"
                      fill="#1e2025"
                      stroke="#f5f7fa"
                      strokeWidth="2.4"
                    />
                  </svg>

                  <div style={{ fontSize: "13.5px", fontWeight: 500 }}>
                    {netVal?.primary_interface || "Home_5G"} ›
                  </div>

                  <div
                    style={{
                      width: "100%",
                      display: "grid",
                      gridTemplateColumns: "1fr 1px 1fr",
                      alignItems: "center",
                      borderTop: "1px solid #363940",
                      borderBottom: "1px solid #363940",
                      padding: "10px 0",
                    }}
                  >
                    <div>
                      <div
                        className="tabular-nums"
                        style={{ fontSize: "18px", fontWeight: 500 }}
                      >
                        {txMbps}
                      </div>
                      <div
                        style={{
                          fontSize: "11px",
                          color: "var(--text-muted)",
                          marginTop: "2px",
                        }}
                      >
                        ↑ Mbps
                      </div>
                    </div>
                    <div
                      style={{
                        width: "1px",
                        height: "28px",
                        background: "#363940",
                      }}
                    />
                    <div>
                      <div
                        className="tabular-nums"
                        style={{ fontSize: "18px", fontWeight: 500 }}
                      >
                        {rxMbps}
                      </div>
                      <div
                        style={{
                          fontSize: "11px",
                          color: "var(--text-muted)",
                          marginTop: "2px",
                        }}
                      >
                        ↓ Mbps
                      </div>
                    </div>
                  </div>

                  <div>
                    <div
                      className="tabular-nums"
                      style={{ fontSize: "18px", fontWeight: 500 }}
                    >
                      {telemetry?.total_processes ?? 227}
                    </div>
                    <div
                      style={{
                        fontSize: "11px",
                        color: "var(--text-muted)",
                        marginTop: "2px",
                      }}
                    >
                      ↔ procs
                    </div>
                  </div>
                </div>
              </div>
            </>
          )}

          {mode === "balls" && (
            <div className="slate-card" style={{ gap: "12px" }}>
              <div className="metric-head-row">
                <div style={{ fontWeight: 600 }}>
                  Disk Space Balls & Largest Folders
                </div>
                <button
                  type="button"
                  className="release-btn"
                  onClick={() => setMode("status")}
                >
                  ← Status
                </button>
              </div>
              <BallsCanvas
                balls={diskSnapshot?.balls ?? []}
                selectedBallId={selectedBall?.id ?? null}
                onSelectBall={(b) => setSelectedBall(b)}
              />
              {selectedBall && (
                <div className="card-bottom-row">
                  <span>
                    <strong>{selectedBall.label}</strong> (
                    {formatBytes(selectedBall.bytes)})
                  </span>
                  <button
                    type="button"
                    className="release-btn"
                    onClick={() =>
                      handleTrash(selectedBall.path, selectedBall.identity)
                    }
                  >
                    Release to Trash
                  </button>
                </div>
              )}
            </div>
          )}

          {mode === "projects" && (
            <div className="slate-card" style={{ gap: "10px" }}>
              <div className="metric-head-row">
                <div style={{ fontWeight: 600 }}>
                  Developer Projects & Build Bloat
                </div>
                <button
                  type="button"
                  className="release-btn"
                  onClick={() => setMode("status")}
                >
                  ← Status
                </button>
              </div>
              {(diskSnapshot?.projects ?? []).map((proj) => (
                <div
                  key={proj.root_path}
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                    padding: "8px 10px",
                    borderRadius: "8px",
                    background: "var(--bg-card-recessed)",
                    fontSize: "12.5px",
                  }}
                >
                  <div>
                    <div style={{ fontWeight: 600 }}>
                      {proj.name} ({proj.kind})
                    </div>
                    <div
                      style={{ fontSize: "11px", color: "var(--text-muted)" }}
                    >
                      {proj.root_path}
                    </div>
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                    <span className="tabular-nums" style={{ color: "var(--green-bar)" }}>
                      {formatBytes(proj.artifact_bytes)}
                    </span>
                    {proj.artifacts[0] && (
                      <button
                        type="button"
                        className="release-btn"
                        onClick={() =>
                          handleTrash(
                            proj.artifacts[0].path,
                            proj.artifacts[0].identity
                          )
                        }
                      >
                        Release {proj.artifacts[0].name}
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </main>

      {/* 4. Bottom Footer */}
      <footer className="window-footer">
        <span>
          {isScanning ? "Scanning filesystem…" : "R1–R4 Protected • Samo"}
        </span>
        <span style={{ display: "inline-flex", alignItems: "center", gap: "10px" }}>
          <span>Version 1.25</span>
          <button
            type="button"
            className="icon-btn"
            aria-label="Rescan storage"
            onClick={() => triggerScan()}
          >
            ↻
          </button>
        </span>
      </footer>
    </div>
  );
};
