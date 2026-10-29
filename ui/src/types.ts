export type FileCategory =
  | "media"
  | "documents"
  | "code"
  | "archives"
  | "applications"
  | "system"
  | "other";

export type PhosphorTheme = "dark" | "light" | "green" | "amber" | "blue" | "mono";

export type Reading<T> =
  | { state: "available"; value: T; sampled_at_ms: number }
  | { state: "unsupported"; reason: string }
  | { state: "stopped"; since_ms: number };

export interface FileNodeIdentity {
  device_id: number;
  inode: number;
}

export interface ProcessIdentity {
  pid: number;
  start_time_secs: number;
}

export interface CategoryBreakdownItem {
  category: FileCategory;
  label: string;
  bytes: number;
  file_count: number;
  share_pct: number;
}

export interface ScanFolderEntry {
  name: string;
  path: string;
  bytes: number;
  file_count: number;
  category: FileCategory;
  identity: FileNodeIdentity;
}

export interface ScanFileEntry {
  name: string;
  path: string;
  bytes: number;
  category: FileCategory;
  identity: FileNodeIdentity;
}

export interface ProjectArtifactItem {
  name: string;
  path: string;
  bytes: number;
  identity: FileNodeIdentity;
}

export interface ProjectSummary {
  name: string;
  root_path: string;
  kind: string;
  total_bytes: number;
  artifact_bytes: number;
  source_bytes: number;
  artifacts: ProjectArtifactItem[];
  identity: FileNodeIdentity;
}

export interface PackedBall {
  id: string;
  label: string;
  path: string;
  bytes: number;
  category: FileCategory;
  x: number;
  y: number;
  r: number;
  identity: FileNodeIdentity;
}

export interface ScanSnapshot {
  scan_id: string;
  root_path: string;
  volume_name: string;
  status: "scanning" | "completed" | "failed" | "interrupted";
  started_at_ms: number;
  completed_at_ms: number | null;
  volume_total_bytes: number;
  volume_used_bytes: number;
  volume_free_bytes: number;
  scanned_bytes: number;
  scanned_files: number;
  unreadable_paths: number;
  categories: CategoryBreakdownItem[];
  largest_folders: ScanFolderEntry[];
  largest_files: ScanFileEntry[];
  projects: ProjectSummary[];
  balls: PackedBall[];
}

export interface ScanHistoryItem {
  scan_id: string;
  root_path: string;
  volume_name: string;
  status: string;
  scanned_bytes: number;
  scanned_files: number;
  volume_used_bytes: number;
  volume_total_bytes: number;
  started_at_ms: number;
  completed_at_ms: number | null;
}

export interface MemoryBreakdown {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  wired_bytes: Reading<number>;
  compressed_bytes: Reading<number>;
  cached_bytes: Reading<number>;
  swap_total_bytes: number;
  swap_used_bytes: number;
  pressure: "normal" | "elevated" | "critical";
}

export interface DiskIoRates {
  read_bytes_per_sec: number;
  write_bytes_per_sec: number;
  total_read_bytes: number;
  total_write_bytes: number;
}

export interface NetworkRates {
  rx_bytes_per_sec: number;
  tx_bytes_per_sec: number;
  total_rx_bytes: number;
  total_tx_bytes: number;
  primary_interface: string;
  local_ip: string;
}

export interface ProcessRow {
  identity: ProcessIdentity;
  parent_pid: number | null;
  name: string;
  executable_path: string;
  user: string;
  cpu_pct: number;
  memory_bytes: number;
  disk_read_bytes_per_sec: number;
  disk_write_bytes_per_sec: number;
  energy_impact: "low" | "moderate" | "high" | "very_high";
  is_self: boolean;
}

export interface SystemSummary {
  hostname: string;
  os_version: string;
  chip_brand: string;
  physical_cores: number;
  logical_cores: number;
  perf_cores: Reading<number>;
  eff_cores: Reading<number>;
  uptime_secs: number;
  boot_reason: string;
}

export interface TelemetryPoint {
  timestamp_ms: number;
  generation: number;
  cpu_overall_pct: Reading<number>;
  cpu_perf_cores_pct: Reading<number[]>;
  cpu_eff_cores_pct: Reading<number[]>;
  memory: Reading<MemoryBreakdown>;
  disk_io: Reading<DiskIoRates>;
  network: Reading<NetworkRates>;
}

export interface TelemetrySnapshot {
  system: SystemSummary;
  latest: TelemetryPoint;
  history: TelemetryPoint[];
  processes: ProcessRow[];
  total_processes: number;
  self_pid: number;
  provider_healthy: boolean;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIdx = 0;
  while (value >= 1024 && unitIdx < units.length - 1) {
    value /= 1024;
    unitIdx++;
  }
  return `${value >= 100 || unitIdx === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIdx]}`;
}

export const CATEGORY_COLORS: Record<FileCategory, string> = {
  media: "#e4685d",
  documents: "#5cc173",
  code: "#5798e6",
  archives: "#e5b84b",
  applications: "#a971e8",
  system: "#808b99",
  other: "#616a75",
};
