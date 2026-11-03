mod analysis;
pub use analysis::*;
use std::{collections::BTreeMap, time::Duration};

#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Int(i64),
    UInt(u64),
    Float(f64),
    Duration(Duration),
    Group(BTreeMap<String, MetricValue>),
    OrderedGroup(Vec<(String, MetricValue)>),
    Samples(Vec<MetricValue>),
}

impl MetricValue {
    pub fn as_scalar(&self) -> Option<f64> {
        let value = match self {
            Self::Int(v) => *v as f64,
            Self::UInt(v) => *v as f64,
            Self::Float(v) => *v,
            Self::Duration(v) => v.as_secs_f64(),
            _ => return None,
        };
        value.is_finite().then_some(value)
    }
    pub fn kind(&self) -> Option<MetricType> {
        match self {
            Self::Int(_) => Some(MetricType::Int),
            Self::UInt(_) => Some(MetricType::UInt),
            Self::Float(_) => Some(MetricType::Float),
            Self::Duration(_) => Some(MetricType::Duration),
            Self::Group(_) | Self::OrderedGroup(_) | Self::Samples(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetricDescriptor {
    Value {
        name: &'static str,
        desc: &'static str,
        kind: MetricType,
        unit: &'static str,
        random: bool,
        direction: Direction,
    },
    Group {
        name: &'static str,
        children: Vec<MetricDescriptor>,
    },
    Map {
        name: &'static str,
        children: Vec<MetricDescriptor>,
    },
    /// Ordered named nodes with scalar fields and an optional `children` tree.
    Tree {
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
            Self::Value { name, .. }
            | Self::Group { name, .. }
            | Self::Map { name, .. }
            | Self::Tree { name, .. }
            | Self::Samples { name, .. } => name,
        }
    }

    pub fn matches_value(&self, value: &MetricValue) -> bool {
        match (self, value) {
            (Self::Value { kind, .. }, value) => Some(kind) == value.kind().as_ref(),
            (Self::Group { children, .. }, MetricValue::Group(values)) => {
                matches_group(children, values)
            }
            (Self::Map { children, .. }, MetricValue::Group(values)) => values.values().all(|value| {
                matches!(value, MetricValue::Group(fields) if matches_group(children, fields))
            }),
            (Self::Tree { children, .. }, MetricValue::OrderedGroup(nodes)) => {
                nodes.iter().all(|(_, node)| {
                    matches!(node, MetricValue::Group(fields) if fields.iter().all(|(name, value)| {
                        if name == "children" {
                            self.matches_value(value)
                        } else {
                            children.iter().any(|child| child.name() == name && child.matches_value(value))
                        }
                    }))
                })
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
            direction: Direction::Unspecified,
        }
    }

    pub fn group(name: &'static str, children: Vec<Self>) -> Self {
        Self::Group { name, children }
    }

    pub fn with_direction(mut self, value: Direction) -> Self {
        if let Self::Value { direction, .. } = &mut self {
            *direction = value;
        }
        self
    }

    /// Named records whose keys are discovered from stored outputs.
    pub fn map(name: &'static str, children: Vec<Self>) -> Self {
        Self::Map { name, children }
    }

    pub fn tree(name: &'static str, children: Vec<Self>) -> Self {
        Self::Tree { name, children }
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
            fn observation_sources(&self) -> Option<Vec<(String, String)>> { Some(vec![]) }
            fn get_scalar(&self, path: &str) -> Option<f64> {
                let (name, rest) = path.split_once('.').map_or((path, None), |(n, p)| (n, Some(p)));
                match name {
                    $(stringify!($field) => $crate::benchmark_metrics!(@scalar self, $form $field (rest) ($($args)*)),)*
                    _ => None,
                }
            }
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
    (@scalar $this:ident, group $field:ident ($rest:ident) ($ty:ty)) => {
        $crate::experiment::BenchmarkOut::get_scalar(&$this.$field, $rest?)
    };
    (@scalar $this:ident, $form:ident $field:ident ($rest:ident) ($($args:tt)*)) => {{
        if $rest.is_some() { None } else {
            $crate::benchmark_metrics!(@value $this, $form $field ($($args)*))?.as_scalar()
        }
    }};
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
