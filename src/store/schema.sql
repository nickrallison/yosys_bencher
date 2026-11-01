PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS metadata (id INTEGER PRIMARY KEY CHECK(id=1), generation INTEGER NOT NULL);
INSERT OR IGNORE INTO metadata VALUES(1,0);
CREATE TABLE IF NOT EXISTS experiments (
 id INTEGER PRIMARY KEY, key TEXT NOT NULL, name TEXT NOT NULL, description TEXT NOT NULL,
 repository TEXT NOT NULL, contract TEXT NOT NULL, created_at INTEGER NOT NULL,
 UNIQUE(repository,key)
);
CREATE TABLE IF NOT EXISTS definitions (
 id INTEGER PRIMARY KEY, experiment INTEGER NOT NULL REFERENCES experiments(id), major INTEGER NOT NULL, minor INTEGER NOT NULL,
 UNIQUE(experiment,major,minor)
);
CREATE TABLE IF NOT EXISTS inputs (
 id INTEGER PRIMARY KEY, procedure TEXT NOT NULL, schema_version INTEGER NOT NULL, payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS input_family ON inputs(procedure,schema_version);
CREATE TABLE IF NOT EXISTS suite_inputs (
 definition INTEGER NOT NULL REFERENCES definitions(id), input INTEGER NOT NULL REFERENCES inputs(id), position INTEGER NOT NULL,
 PRIMARY KEY(definition,input), UNIQUE(definition,position)
);
CREATE TABLE IF NOT EXISTS revisions (
 experiment INTEGER NOT NULL REFERENCES experiments(id), revision TEXT NOT NULL, payload TEXT NOT NULL,
 PRIMARY KEY(experiment,revision)
);
CREATE TABLE IF NOT EXISTS runs (
 id INTEGER PRIMARY KEY, experiment INTEGER NOT NULL REFERENCES experiments(id), definition INTEGER NOT NULL REFERENCES definitions(id),
 plan TEXT NOT NULL, partition_key TEXT NOT NULL, state TEXT NOT NULL, started_at INTEGER NOT NULL, finished_at INTEGER
);
CREATE INDEX IF NOT EXISTS run_selection ON runs(experiment,definition,partition_key);
CREATE TABLE IF NOT EXISTS run_revisions (
 run INTEGER NOT NULL REFERENCES runs(id), revision TEXT NOT NULL, position INTEGER NOT NULL,
 state TEXT NOT NULL, reason TEXT, PRIMARY KEY(run,revision), UNIQUE(run,position)
);
CREATE TABLE IF NOT EXISTS work_items (
 id INTEGER PRIMARY KEY, run INTEGER NOT NULL REFERENCES runs(id), revision TEXT NOT NULL, input INTEGER NOT NULL REFERENCES inputs(id),
 state TEXT NOT NULL, reason TEXT, planned_repetitions INTEGER NOT NULL, UNIQUE(run,revision,input)
);
CREATE INDEX IF NOT EXISTS work_selection ON work_items(run,revision,input);
CREATE TABLE IF NOT EXISTS attempts (
 id INTEGER PRIMARY KEY, work_item INTEGER NOT NULL REFERENCES work_items(id), sample INTEGER NOT NULL,
 state TEXT NOT NULL, started_at INTEGER NOT NULL, outcome TEXT, artifact_state TEXT NOT NULL DEFAULT 'pending', record TEXT, UNIQUE(work_item,sample)
);
CREATE TABLE IF NOT EXISTS artifacts (
 id INTEGER PRIMARY KEY, attempt INTEGER NOT NULL REFERENCES attempts(id), name TEXT NOT NULL, path TEXT NOT NULL
);
PRAGMA user_version = 1;
