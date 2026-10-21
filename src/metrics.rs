use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Int(i64),
    UInt(u64),
    Float(f64),
    Duration(Duration),
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
    #[get]
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
