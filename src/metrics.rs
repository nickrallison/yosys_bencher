use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Int(i64),
    UInt(u64),
    Float(f64),
    Duration(Duration),
}

impl MetricValue {
    pub fn kind(&self) -> MetricType {
        match self {
            Self::Int(_) => MetricType::Int,
            Self::UInt(_) => MetricType::UInt,
            Self::Float(_) => MetricType::Float,
            Self::Duration(_) => MetricType::Duration,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetricType {
    Int,
    UInt,
    Float,
    Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Getters)]
pub struct MetricDescriptor {
    #[get = "pub"]
    name: &'static str,
    #[get]
    desc: &'static str,
    #[get]
    kind: MetricType,
    #[get]
    unit: &'static str,
    #[get]
    random: bool,
}

impl MetricDescriptor {
    pub fn matches_value(&self, value: &MetricValue) -> bool {
        self.kind == value.kind()
    }

    pub fn new(
        name: &'static str,
        desc: &'static str,
        kind: MetricType,
        unit: &'static str,
        random: bool,
    ) -> Self {
        Self {
            name,
            desc,
            kind,
            unit,
            random,
        }
    }
}
