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
    pub fn kind(&self) -> Option<MetricType> {
        match self {
            Self::Int(_) => Some(MetricType::Int),
            Self::UInt(_) => Some(MetricType::UInt),
            Self::Float(_) => Some(MetricType::Float),
            Self::Duration(_) => Some(MetricType::Duration),
            Self::Group(_) | Self::Samples(_) => None,
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
            (Self::Value { kind, .. }, value) => Some(kind) == value.kind().as_ref(),
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

fn matches_group(children: &[MetricDescriptor], values: &BTreeMap<String, MetricValue>) -> bool {
    values.iter().all(|(name, value)| {
        children
            .iter()
            .any(|child| child.name() == name && child.matches_value(value))
    })
}

#[macro_export]
macro_rules! benchmark_metrics {
    ($ty:ty { $( $form:ident $field:ident ( $($args:tt)* ); )* }) => {
        #[contracts::contract_trait]
        impl $crate::experiment::BenchmarkOut for $ty {
            fn list_metrics() -> Vec<$crate::metrics::MetricDescriptor> {
                vec![$($crate::benchmark_metrics!(@descriptor $form $field ($($args)*))),*]
            }
            fn get_metric(&self, name: &str) -> Option<$crate::metrics::MetricValue> {
                match name {
                    $(stringify!($field) => $crate::benchmark_metrics!(@value self, $form $field ($($args)*)),)*
                    _ => None,
                }
            }
        }
    };
    (@descriptor group $field:ident ($ty:ty)) => {
        $crate::metrics::MetricDescriptor::group(stringify!($field), <$ty as $crate::experiment::BenchmarkOut>::list_metrics())
    };
    (@descriptor $form:ident $field:ident ($kind:ident, $desc:expr, $unit:expr, $random:expr $(, $get:expr)?)) => {
        $crate::metrics::MetricDescriptor::new(stringify!($field), $desc, $crate::metrics::MetricType::$kind, $unit, $random)
    };
    (@value $this:ident, group $field:ident ($ty:ty)) => {
        Some($crate::experiment::BenchmarkOut::metrics(&$this.$field))
    };
    (@value $this:ident, scalar $field:ident ($kind:ident, $desc:expr, $unit:expr, $random:expr)) => {
        Some($crate::metrics::MetricValue::$kind($this.$field.into()))
    };
    (@value $this:ident, optional $field:ident ($kind:ident, $desc:expr, $unit:expr, $random:expr)) => {
        $this.$field.map(|value| $crate::metrics::MetricValue::$kind(value.into()))
    };
    (@value $this:ident, computed $field:ident ($kind:ident, $desc:expr, $unit:expr, $random:expr, $get:expr)) => {
        ($get)($this).map($crate::metrics::MetricValue::$kind)
    };
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
