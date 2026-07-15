export type Availability =
  | { state: "available" }
  | { state: "permission_denied"; reason: string }
  | { state: "unsupported"; reason: string }
  | { state: "failed"; reason: string };

export interface ServiceStatus {
  service_version: string;
  started_at: string;
  latest_sample_at: string | null;
  samples_collected: number;
  dropped_samples: number;
  sampling_interval_ms: number;
  database_bytes: number;
  active_experiments: number;
}

export interface ProcessMetrics {
  identity: { pid: number; creation_time_100ns: string };
  executable_name: string;
  executable_path: string | null;
  cpu_percent: number | null;
  working_set_bytes: number | null;
  private_bytes: number | null;
  handle_count: number | null;
  thread_count: number | null;
  inaccessible_reason: string | null;
}

export interface MetricSnapshot {
  sequence: number;
  captured_at: string;
  collection_duration_us: number;
  sampling_interval_ms: number;
  dropped_samples: number;
  cpu: { total_percent: number | null; logical_processor_count: number };
  memory: {
    total_physical_bytes: number;
    available_physical_bytes: number;
    committed_bytes: number;
    commit_limit_bytes: number;
    memory_load_percent: number;
  };
  processes: ProcessMetrics[];
  capabilities: Record<string, Availability>;
}

export interface DoctorReport {
  healthy: boolean;
  checks: Array<{
    name: string;
    status: "pass" | "warning" | "fail";
    details: string;
  }>;
}
