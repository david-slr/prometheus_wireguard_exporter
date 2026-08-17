use super::metric_type::MetricType;
use super::prometheus_metric::PrometheusMetric;
use super::{No, ToAssign, Yes};
use std::marker::PhantomData;

pub(crate) struct PrometheusMetricBuilder<'a, Name, Type, Help> {
    name: Option<&'a str>,
    metric_type: Option<MetricType>,
    help: Option<&'a str>,
    state: PhantomData<(Name, Type, Help)>,
}

impl<'a> PrometheusMetricBuilder<'a, No, No, No> {
    pub(crate) fn new() -> Self {
        Self {
            name: None,
            metric_type: None,
            help: None,
            state: PhantomData,
        }
    }
}

impl<'a, Name, Type, Help> PrometheusMetricBuilder<'a, Name, Type, Help> {
    fn name(&self) -> &'a str {
        self.name.expect("metric name must be assigned")
    }

    fn metric_type(&self) -> MetricType {
        self.metric_type.expect("metric type must be assigned")
    }

    fn help(&self) -> &'a str {
        self.help.expect("metric help must be assigned")
    }
}

impl<'a, Name, Type, Help> PrometheusMetricBuilder<'a, Name, Type, Help>
where
    Name: ToAssign,
{
    pub(crate) fn with_name(
        mut self,
        name: &'a str,
    ) -> PrometheusMetricBuilder<'a, Name::Assigned, Type, Help> {
        self.name = Some(name);
        PrometheusMetricBuilder {
            name: self.name,
            metric_type: self.metric_type,
            help: self.help,
            state: PhantomData,
        }
    }
}

impl<'a, Name, Type, Help> PrometheusMetricBuilder<'a, Name, Type, Help>
where
    Type: ToAssign,
{
    pub(crate) fn with_metric_type(
        mut self,
        metric_type: MetricType,
    ) -> PrometheusMetricBuilder<'a, Name, Type::Assigned, Help> {
        self.metric_type = Some(metric_type);
        PrometheusMetricBuilder {
            name: self.name,
            metric_type: self.metric_type,
            help: self.help,
            state: PhantomData,
        }
    }
}

impl<'a, Name, Type, Help> PrometheusMetricBuilder<'a, Name, Type, Help>
where
    Help: ToAssign,
{
    pub(crate) fn with_help(
        mut self,
        help: &'a str,
    ) -> PrometheusMetricBuilder<'a, Name, Type, Help::Assigned> {
        self.help = Some(help);
        PrometheusMetricBuilder {
            name: self.name,
            metric_type: self.metric_type,
            help: self.help,
            state: PhantomData,
        }
    }
}

impl<'a> PrometheusMetricBuilder<'a, Yes, Yes, Yes> {
    pub(crate) fn build(self) -> PrometheusMetric<'a> {
        PrometheusMetric {
            counter_name: self.name(),
            counter_type: self.metric_type(),
            counter_help: self.help(),
            rendered_instances: Vec::new(),
        }
    }
}
