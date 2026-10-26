use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, de::Error};

use super::{BenchmarkIn, ExperimentError, Procedure, ProcedureId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct Version {
    major: u32,
    minor: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new(1, 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpSuite {
    Eq,
    Superset,
    Incompatible,
}

pub trait Suite {
    type Tool;
    type Session;
    type Procedure: Procedure;

    fn name(&self) -> &str;
    fn procedure(&self) -> &Self::Procedure;
    fn inputs(&self) -> Result<Vec<<Self::Procedure as Procedure>::Input>, ExperimentError>;
    fn begin(&self) -> Result<Self::Session, ExperimentError>;
    fn tools(
        &self,
        tool: &Self::Tool,
        session: &Self::Session,
    ) -> Result<<Self::Procedure as Procedure>::Tools, ExperimentError>;

    fn definition(
        &self,
    ) -> Result<SuiteDefinition<<Self::Procedure as Procedure>::Input>, ExperimentError> {
        SuiteDefinition::new(
            ProcedureId::new(self.procedure().name(), Version::default()),
            self.inputs()?,
        )
    }
}

#[derive(Debug, Clone, Serialize, Getters)]
#[getset(get = "pub")]
pub struct SuiteDefinition<I> {
    procedure: ProcedureId,
    inputs: Vec<I>,
}

impl<I: BenchmarkIn> SuiteDefinition<I> {
    pub fn new(procedure: ProcedureId, inputs: Vec<I>) -> Result<Self, ExperimentError> {
        let mut seen = HashSet::new();
        for (index, input) in inputs.iter().enumerate() {
            if !seen.insert(input) {
                return Err(ExperimentError::DuplicateInput { index });
            }
        }
        Ok(Self { procedure, inputs })
    }

    pub fn compare(&self, previous: &Self) -> CmpSuite {
        if self.procedure.name() != previous.procedure.name() {
            return CmpSuite::Incompatible;
        }
        let current: HashSet<_> = self.inputs.iter().collect();
        let previous: HashSet<_> = previous.inputs.iter().collect();
        if current == previous {
            CmpSuite::Eq
        } else if current.is_superset(&previous) {
            CmpSuite::Superset
        } else {
            CmpSuite::Incompatible
        }
    }

    pub fn version(&self) -> &Version {
        self.procedure.version()
    }

    pub fn with_previous(mut self, previous: &Self) -> Self {
        let version = self.next_version(previous);
        self.procedure = ProcedureId::new(self.procedure.name().clone(), version);
        self
    }

    pub fn next_version(&self, previous: &Self) -> Version {
        let version = *previous.version();
        match self.compare(previous) {
            CmpSuite::Eq => version,
            CmpSuite::Superset => Version::new(version.major, version.minor + 1),
            CmpSuite::Incompatible => {
                let next = Version::new(version.major + 1, 0);
                log::warn!(
                    "incompatible suite change; bumping major version from {}.{} to {}.0",
                    version.major, version.minor, next.major
                );
                next
            }
        }
    }
}

impl<'de, I: BenchmarkIn> Deserialize<'de> for SuiteDefinition<I> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Definition<I> {
            procedure: ProcedureId,
            inputs: Vec<I>,
        }

        let definition = Definition::deserialize(deserializer)?;
        Self::new(definition.procedure, definition.inputs).map_err(D::Error::custom)
    }
}
