use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricType {
    Counter,
    Gauge,
    #[allow(dead_code)]
    Histogram,
    #[allow(dead_code)]
    Summary,
}

impl AsRef<str> for MetricType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
            Self::Summary => "summary",
        }
    }
}

impl Display for MetricType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        formatter.write_str(self.as_ref())
    }
}
