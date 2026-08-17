use super::No;
use super::metric_type::MetricType;
use super::prometheus_metric_builder::PrometheusMetricBuilder;
use super::render_to_prometheus::RenderToPrometheus;

pub(crate) struct PrometheusMetric<'a> {
    pub(crate) counter_name: &'a str,
    pub(crate) counter_type: MetricType,
    pub(crate) counter_help: &'a str,
    pub(crate) rendered_instances: Vec<String>,
}

impl<'a> PrometheusMetric<'a> {
    pub(crate) fn build() -> PrometheusMetricBuilder<'a, No, No, No> {
        PrometheusMetricBuilder::new()
    }

    pub(crate) fn render_and_append_instance(
        &mut self,
        instance: &dyn RenderToPrometheus,
    ) -> &mut Self {
        self.rendered_instances
            .push(format!("{}{}\n", self.counter_name, instance.render()));
        self
    }

    fn render_header(&self) -> String {
        format!(
            "# HELP {} {}\n# TYPE {} {}\n",
            self.counter_name, self.counter_help, self.counter_name, self.counter_type
        )
    }

    pub(crate) fn render(&self) -> String {
        let mut rendered = self.render_header();
        for instance in &self.rendered_instances {
            rendered.push_str(instance);
        }
        rendered
    }
}
