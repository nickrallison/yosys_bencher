use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Number,
    Text,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputField {
    pub path: String,
    pub label: String,
    pub kind: FieldKind,
    pub groupable: bool,
}

impl FieldKind {
    pub fn operators(self) -> Vec<FilterOperator> {
        use FilterOperator::*;
        let mut ops = vec![Equal, NotEqual, Present, Missing];
        match self {
            Self::Number => ops.extend([Less, AtMost, Greater, AtLeast]),
            Self::Text => ops.push(Contains),
            Self::Boolean => (),
        }
        ops
    }
}
impl InputField {
    pub fn new(path: &str, label: &str, kind: FieldKind) -> Self {
        Self {
            path: path.into(),
            label: label.into(),
            kind,
            groupable: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Equal,
    NotEqual,
    Less,
    AtMost,
    Greater,
    AtLeast,
    Contains,
    Present,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub operator: FilterOperator,
    pub value: Value,
}

impl Filter {
    pub fn matches(&self, value: Option<&Value>) -> bool {
        let value = value.filter(|v| !v.is_null());
        match self.operator {
            FilterOperator::Present => value.is_some(),
            FilterOperator::Missing => value.is_none(),
            FilterOperator::Equal => {
                value.is_some_and(|v| match (v.as_f64(), self.value.as_f64()) {
                    (Some(a), Some(b)) => a == b,
                    _ => v == &self.value,
                })
            }
            FilterOperator::NotEqual => {
                value.is_some_and(|v| match (v.as_f64(), self.value.as_f64()) {
                    (Some(a), Some(b)) => a != b,
                    _ => v != &self.value,
                })
            }
            FilterOperator::Contains => value
                .and_then(Value::as_str)
                .zip(self.value.as_str())
                .is_some_and(|(a, b)| a.contains(b)),
            op => value
                .and_then(Value::as_f64)
                .zip(self.value.as_f64())
                .is_some_and(|(a, b)| match op {
                    FilterOperator::Less => a < b,
                    FilterOperator::AtMost => a <= b,
                    FilterOperator::Greater => a > b,
                    FilterOperator::AtLeast => a >= b,
                    _ => false,
                }),
        }
    }

    pub fn validate(&self, fields: &[InputField]) -> Result<(), String> {
        let field = fields
            .iter()
            .find(|f| f.path == self.field)
            .ok_or_else(|| format!("Unknown field: {}", self.field))?;
        if matches!(
            self.operator,
            FilterOperator::Present | FilterOperator::Missing
        ) {
            return Ok(());
        }
        let valid_value = match field.kind {
            FieldKind::Number => self.value.as_f64().is_some_and(f64::is_finite),
            FieldKind::Text => self.value.is_string(),
            FieldKind::Boolean => self.value.is_boolean(),
        };
        let valid_op = match self.operator {
            FilterOperator::Equal | FilterOperator::NotEqual => true,
            FilterOperator::Contains => field.kind == FieldKind::Text,
            _ => field.kind == FieldKind::Number,
        };
        if valid_value && valid_op {
            Ok(())
        } else {
            Err(format!("Invalid filter for {}", self.field))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDescription {
    pub id: crate::store::InputId,
    pub label: String,
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub total: usize,
    pub offset: usize,
    pub items: Vec<T>,
}

impl<T> Page<T> {
    pub fn from_items(items: impl IntoIterator<Item = T>, offset: usize, limit: usize) -> Self {
        let items: Vec<_> = items.into_iter().collect();
        Self {
            total: items.len(),
            offset,
            items: items.into_iter().skip(offset).take(limit).collect(),
        }
    }
}

use std::time::{Duration, SystemTime};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttemptOutcome<O> {
    Success(O),
    Failure(String),
    Unsupported(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord<O> {
    pub started_at: SystemTime,
    pub duration: Duration,
    pub outcome: AttemptOutcome<O>,
    pub diagnostics: Vec<String>,
}
