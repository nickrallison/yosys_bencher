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
