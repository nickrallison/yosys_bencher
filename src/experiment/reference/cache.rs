use super::*;
use std::collections::HashMap;

pub struct ReferenceMetadata {
    pub executable: ExecutableId,
    pub os: String,
    pub arch: String,
}

pub struct ReferenceObservation<M> {
    pub measured_at: SystemTime,
    pub measurements: M,
    pub perf: Option<PerfMeasurements>,
}

pub struct ReferenceCache<K, M> {
    metadata: ReferenceMetadata,
    references: HashMap<(K, u32), Reference<M>>,
}

impl<K: Eq + Hash, M: Clone> ReferenceCache<K, M> {
    pub fn new(metadata: ReferenceMetadata) -> Self {
        Self {
            metadata,
            references: HashMap::new(),
        }
    }

    pub fn identity(&self) -> &ExecutableId {
        &self.metadata.executable
    }

    pub fn validate_identity(&self, expected: &ExecutableId, current: &ExecutableId) -> Result<()> {
        if expected != self.identity() || current != self.identity() {
            return Err(ExperimentError::Config(
                "reference executable differs from the workload or changed during the invocation"
                    .into(),
            ));
        }
        Ok(())
    }

    pub fn get(&self, key: &(K, u32)) -> Option<&Reference<M>> {
        self.references.get(key)
    }

    pub fn record(
        &mut self,
        key: (K, u32),
        observation: Result<ReferenceObservation<M>>,
    ) -> Result<Reference<M>> {
        let observation = observation?;
        if let Some(reference) = self.references.get(&key) {
            return Ok(reference.clone());
        }
        let reference = Reference {
            id: self.references.len(),
            executable: self.metadata.executable.clone(),
            os: self.metadata.os.clone(),
            arch: self.metadata.arch.clone(),
            measured_at: observation.measured_at,
            measurements: observation.measurements,
            perf: observation.perf,
        };
        self.references.insert(key, reference.clone());
        Ok(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> ReferenceCache<&'static str, u32> {
        ReferenceCache::new(ReferenceMetadata {
            executable: ExecutableId::from_bytes(b"original"),
            os: "test".into(),
            arch: "test".into(),
        })
    }

    #[test]
    fn failures_are_not_stored_and_ids_are_local_to_each_cache() {
        let mut first = cache();
        assert!(
            first
                .record(
                    ("workload", 0),
                    Err(ExperimentError::Parse("failure".into()))
                )
                .is_err()
        );
        assert!(first.get(&("workload", 0)).is_none());
        let observation = || {
            Ok(ReferenceObservation {
                measured_at: SystemTime::UNIX_EPOCH,
                measurements: 7,
                perf: None,
            })
        };
        let reference = first.record(("workload", 0), observation()).unwrap();
        assert_eq!(reference.id, 0);
        assert_eq!(reference.measured_at, SystemTime::UNIX_EPOCH);
        assert_eq!(first.get(&("workload", 0)).unwrap().id, reference.id);
        assert!(first.get(&("other", 0)).is_none());
        assert!(first.get(&("workload", 1)).is_none());
        let sample = first.record(("workload", 1), observation()).unwrap();
        assert_eq!(sample.id, 1);
        let mut fresh = cache();
        assert!(fresh.get(&("workload", 0)).is_none());
        let fresh_reference = fresh.record(("workload", 0), observation()).unwrap();
        assert_eq!(fresh_reference.id, 0);
        first
            .validate_identity(
                &ExecutableId::from_bytes(b"original"),
                &ExecutableId::from_bytes(b"original"),
            )
            .unwrap();
        for (expected, current) in [
            (b"original".as_slice(), b"changed".as_slice()),
            (b"changed".as_slice(), b"original".as_slice()),
        ] {
            assert!(
                first
                    .validate_identity(
                        &ExecutableId::from_bytes(expected),
                        &ExecutableId::from_bytes(current)
                    )
                    .is_err()
            );
        }
    }
}
