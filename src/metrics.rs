#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Int(i64),
    UInt(u64),
    Float(f64),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetricType {
    Int,
    UInt,
    Float,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Getters)]
pub struct MetricDescriptor {
    name: &'static str,
    desc: &'static str,
    kind: MetricType,
    unit: &'static str,
    random: bool,
}
