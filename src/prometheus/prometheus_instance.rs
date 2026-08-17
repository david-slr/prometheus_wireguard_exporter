use super::render_to_prometheus::RenderToPrometheus;
use super::{No, Yes};
use num::Num;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub(crate) struct PrometheusInstance<'a, T = MissingValue, State = No> {
    labels: Vec<(&'a str, &'a str)>,
    value: T,
    timestamp: Option<u128>,
    state: PhantomData<State>,
}

impl<'a> PrometheusInstance<'a, MissingValue, No> {
    pub(crate) fn new() -> Self {
        Self {
            labels: Vec::new(),
            value: MissingValue,
            timestamp: None,
            state: PhantomData,
        }
    }
}

impl<'a, T, State> PrometheusInstance<'a, T, State> {
    pub(crate) fn with_label(mut self, label: &'a str, value: &'a str) -> Self {
        self.labels.push((label, value));
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_timestamp(mut self, timestamp: u128) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_current_timestamp(mut self) -> Result<Self, SystemTimeError> {
        self.timestamp = Some(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis());
        Ok(self)
    }
}

impl<'a, T> PrometheusInstance<'a, T, No> {
    pub(crate) fn with_value<V: Num>(self, value: V) -> PrometheusInstance<'a, V, Yes> {
        PrometheusInstance {
            labels: self.labels,
            value,
            timestamp: self.timestamp,
            state: PhantomData,
        }
    }
}

impl<'a, T> RenderToPrometheus for PrometheusInstance<'a, T, Yes>
where
    T: Debug + Display + Num,
{
    fn render(&self) -> String {
        let labels = self
            .labels
            .iter()
            .map(|(label, value)| format!("{}=\"{}\"", label, value))
            .collect::<Vec<String>>()
            .join(",");
        let labels = if labels.is_empty() {
            String::new()
        } else {
            format!("{{{}}}", labels)
        };
        let timestamp = self
            .timestamp
            .map_or_else(String::new, |timestamp| format!(" {}", timestamp));

        format!("{} {}{}", labels, self.value, timestamp)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MissingValue;
