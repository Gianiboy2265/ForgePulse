CREATE TABLE metric_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    series TEXT NOT NULL,
    start_time_ms INTEGER NOT NULL,
    end_time_ms INTEGER NOT NULL,
    sample_count INTEGER NOT NULL CHECK (sample_count > 0),
    schema_version INTEGER NOT NULL,
    codec TEXT NOT NULL CHECK (codec IN ('msgpack+zstd')),
    checksum_sha256 TEXT NOT NULL,
    payload BLOB NOT NULL,
    created_at_ms INTEGER NOT NULL,
    UNIQUE(series, start_time_ms, end_time_ms)
);
CREATE INDEX idx_metric_chunks_range ON metric_chunks(series, start_time_ms, end_time_ms);

CREATE TABLE processes (
    pid INTEGER NOT NULL,
    creation_time_100ns INTEGER NOT NULL,
    executable_path TEXT,
    executable_name TEXT NOT NULL,
    publisher TEXT,
    signature_status TEXT,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL,
    PRIMARY KEY(pid, creation_time_100ns)
);

CREATE TABLE process_events (
    id TEXT PRIMARY KEY,
    occurred_at_ms INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    pid INTEGER NOT NULL,
    creation_time_100ns INTEGER NOT NULL,
    parent_pid INTEGER,
    exit_code INTEGER,
    properties_json TEXT NOT NULL,
    FOREIGN KEY(pid, creation_time_100ns) REFERENCES processes(pid, creation_time_100ns)
);
CREATE INDEX idx_process_events_time ON process_events(occurred_at_ms);

CREATE TABLE services (
    service_name TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    executable_path TEXT,
    service_account TEXT,
    startup_type TEXT,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL
);

CREATE TABLE service_events (
    id TEXT PRIMARY KEY,
    service_name TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    details_json TEXT NOT NULL,
    FOREIGN KEY(service_name) REFERENCES services(service_name)
);
CREATE INDEX idx_service_events_time ON service_events(occurred_at_ms);

CREATE TABLE incidents (
    id TEXT PRIMARY KEY,
    incident_type TEXT NOT NULL,
    start_time_ms INTEGER NOT NULL,
    end_time_ms INTEGER,
    severity TEXT NOT NULL,
    confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
    causality TEXT NOT NULL,
    summary TEXT NOT NULL,
    affected_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_incidents_start ON incidents(start_time_ms);

CREATE TABLE evidence (
    id TEXT PRIMARY KEY,
    incident_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    weight REAL NOT NULL CHECK(weight >= 0.0 AND weight <= 1.0),
    metric TEXT NOT NULL,
    observation TEXT NOT NULL,
    data_json TEXT NOT NULL,
    FOREIGN KEY(incident_id) REFERENCES incidents(id) ON DELETE CASCADE
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    session_type TEXT NOT NULL,
    start_time_ms INTEGER NOT NULL,
    end_time_ms INTEGER,
    notes TEXT,
    context_json TEXT NOT NULL
);
CREATE INDEX idx_sessions_start ON sessions(start_time_ms);

CREATE TABLE benchmarks (
    id TEXT PRIMARY KEY,
    session_id TEXT,
    benchmark_type TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    configuration_json TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);

CREATE TABLE benchmark_samples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    benchmark_id TEXT NOT NULL,
    metric TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    value REAL NOT NULL,
    captured_at_ms INTEGER NOT NULL,
    FOREIGN KEY(benchmark_id) REFERENCES benchmarks(id) ON DELETE CASCADE,
    UNIQUE(benchmark_id, metric, sequence)
);

CREATE TABLE experiments (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    hypothesis TEXT NOT NULL,
    target_metric TEXT NOT NULL,
    specification_json TEXT NOT NULL,
    risk TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE experiment_runs (
    id TEXT PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    state TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    error TEXT,
    FOREIGN KEY(experiment_id) REFERENCES experiments(id)
);

CREATE TABLE experiment_results (
    id TEXT PRIMARY KEY,
    experiment_run_id TEXT NOT NULL,
    verdict TEXT NOT NULL,
    confidence REAL NOT NULL,
    effect_size REAL,
    result_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(experiment_run_id) REFERENCES experiment_runs(id) ON DELETE CASCADE
);

CREATE TABLE rollback_records (
    id TEXT PRIMARY KEY,
    experiment_run_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('pending','armed','restoring','restored','verification_failed')),
    action_type TEXT NOT NULL,
    action_json TEXT NOT NULL,
    original_state_hash TEXT NOT NULL,
    verification_details TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY(experiment_run_id) REFERENCES experiment_runs(id)
);
CREATE INDEX idx_rollback_state ON rollback_records(state);

CREATE TABLE profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    profile_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE automation_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    rule_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE system_changes (
    id TEXT PRIMARY KEY,
    occurred_at_ms INTEGER NOT NULL,
    source TEXT NOT NULL,
    category TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    reversible INTEGER NOT NULL CHECK(reversible IN (0,1))
);
CREATE INDEX idx_system_changes_time ON system_changes(occurred_at_ms);

CREATE TABLE hardware_devices (
    id TEXT PRIMARY KEY,
    stable_instance_id TEXT NOT NULL,
    device_class TEXT NOT NULL,
    vendor TEXT,
    model TEXT NOT NULL,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL,
    properties_json TEXT NOT NULL
);

CREATE TABLE driver_changes (
    id TEXT PRIMARY KEY,
    hardware_device_id TEXT,
    occurred_at_ms INTEGER NOT NULL,
    previous_version TEXT,
    new_version TEXT,
    details_json TEXT NOT NULL,
    FOREIGN KEY(hardware_device_id) REFERENCES hardware_devices(id)
);

CREATE TABLE application_versions (
    executable_identity TEXT NOT NULL,
    version TEXT NOT NULL,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL,
    PRIMARY KEY(executable_identity, version)
);

CREATE TABLE windows_updates (
    update_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    installed_at_ms INTEGER,
    first_seen_ms INTEGER NOT NULL,
    details_json TEXT NOT NULL
);

CREATE TABLE user_notes (
    id TEXT PRIMARY KEY,
    created_at_ms INTEGER NOT NULL,
    start_time_ms INTEGER,
    end_time_ms INTEGER,
    body TEXT NOT NULL,
    label TEXT
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

