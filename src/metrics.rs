use std::{collections::BTreeMap, time::Duration};

#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Int(i64),
    UInt(u64),
    Float(f64),
    Duration(Duration),
    Group(BTreeMap<String, MetricValue>),
    Samples(Vec<MetricValue>),
}

impl MetricValue {
    pub fn kind(&self) -> MetricType {
        match self {
            Self::Int(_) => MetricType::Int,
            Self::UInt(_) => MetricType::UInt,
            Self::Float(_) => MetricType::Float,
            Self::Duration(_) => MetricType::Duration,
            Self::Group(_) => MetricType::Group,
            Self::Samples(_) => MetricType::Samples,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetricType {
    Int,
    UInt,
    Float,
    Duration,
    Group,
    Samples,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetricDescriptor {
    Value {
        name: &'static str,
        desc: &'static str,
        kind: MetricType,
        unit: &'static str,
        random: bool,
    },
    Group {
        name: &'static str,
        children: Vec<MetricDescriptor>,
    },
    Samples {
        name: &'static str,
        children: Vec<MetricDescriptor>,
    },
}

impl MetricDescriptor {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Value { name, .. } | Self::Group { name, .. } | Self::Samples { name, .. } => {
                name
            }
        }
    }

    pub fn matches_value(&self, value: &MetricValue) -> bool {
        match (self, value) {
            (Self::Value { kind, .. }, value) => *kind == value.kind(),
            (Self::Group { children, .. }, MetricValue::Group(values)) => {
                matches_group(children, values)
            }
            (Self::Samples { children, .. }, MetricValue::Samples(samples)) => {
                samples.iter().all(|sample| match sample {
                    MetricValue::Group(values) => matches_group(children, values),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    pub fn new(
        name: &'static str,
        desc: &'static str,
        kind: MetricType,
        unit: &'static str,
        random: bool,
    ) -> Self {
        Self::Value {
            name,
            desc,
            kind,
            unit,
            random,
        }
    }

    pub fn group(name: &'static str, children: Vec<Self>) -> Self {
        Self::Group { name, children }
    }

    pub fn samples(name: &'static str, children: Vec<Self>) -> Self {
        Self::Samples { name, children }
    }
}

// Missing optional metrics are allowed; every reported value must match its schema.
fn matches_group(children: &[MetricDescriptor], values: &BTreeMap<String, MetricValue>) -> bool {
    values.iter().all(|(name, value)| {
        children
            .iter()
            .any(|child| child.name() == name && child.matches_value(value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(name: &str, value: MetricValue) -> MetricValue {
        MetricValue::Group(BTreeMap::from([(name.into(), value)]))
    }

    #[test]
    fn nested_schemas_validate_each_sample_and_allow_missing_optional_values() {
        let schema = MetricDescriptor::samples(
            "runs",
            vec![MetricDescriptor::group(
                "measurements",
                vec![MetricDescriptor::new(
                    "depth",
                    "Depth",
                    MetricType::UInt,
                    "levels",
                    false,
                )],
            )],
        );
        let sample = |value| group("measurements", value);
        assert!(schema.matches_value(&MetricValue::Samples(vec![
            sample(group("depth", MetricValue::UInt(3))),
            sample(MetricValue::Group(BTreeMap::new())),
        ])));
        for invalid in [
            MetricValue::UInt(3),
            sample(group("depth", MetricValue::Float(3.0))),
            sample(group("unknown", MetricValue::UInt(3))),
            sample(MetricValue::Samples(Vec::new())),
        ] {
            assert!(!schema.matches_value(&MetricValue::Samples(vec![invalid])));
        }
        assert!(!schema.matches_value(&MetricValue::Group(BTreeMap::new())));
        assert!(schema.matches_value(&MetricValue::Samples(Vec::new())));
    }
}
