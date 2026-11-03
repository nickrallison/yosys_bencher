CREATE TABLE IF NOT EXISTS analysis_state (
 experiment INTEGER PRIMARY KEY REFERENCES experiments(id), version INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS analysis_inputs (
 input INTEGER PRIMARY KEY REFERENCES inputs(id), version INTEGER NOT NULL,
 label TEXT NOT NULL, parameters TEXT NOT NULL, warnings TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS analysis_metrics (
 id INTEGER PRIMARY KEY, experiment INTEGER NOT NULL REFERENCES experiments(id),
 path TEXT NOT NULL, position INTEGER NOT NULL, spec TEXT NOT NULL,
 UNIQUE(experiment,path), UNIQUE(experiment,position)
);
CREATE TABLE IF NOT EXISTS analysis_records (
 attempt INTEGER PRIMARY KEY REFERENCES attempts(id), summary TEXT NOT NULL, sources TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS analysis_values (
 attempt INTEGER NOT NULL REFERENCES attempts(id),
 metric INTEGER NOT NULL REFERENCES analysis_metrics(id) ON DELETE CASCADE,
 value REAL, identity TEXT, PRIMARY KEY(attempt,metric)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS analysis_metric_values ON analysis_values(metric,attempt);
PRAGMA user_version = 2;
