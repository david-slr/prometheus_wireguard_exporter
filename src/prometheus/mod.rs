mod metric_type;
mod prometheus_instance;
mod prometheus_metric;
mod prometheus_metric_builder;
mod render_to_prometheus;

pub(crate) use metric_type::MetricType;
pub(crate) use prometheus_instance::PrometheusInstance;
pub(crate) use prometheus_metric::PrometheusMetric;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Yes;
#[derive(Debug, Clone, Copy)]
pub(crate) struct No;

pub(crate) trait ToAssign {
    type Assigned;
}

impl ToAssign for No {
    type Assigned = Yes;
}

#[cfg(test)]
mod tests {
    use super::*;
    use render_to_prometheus::RenderToPrometheus;

    #[test]
    fn renders_metric_header() {
        let metric = PrometheusMetric::build()
            .with_name("wireguard_test_total")
            .with_metric_type(MetricType::Counter)
            .with_help("Test metric")
            .build();

        assert_eq!(
            metric.render(),
            "# HELP wireguard_test_total Test metric\n# TYPE wireguard_test_total counter\n"
        );
    }

    #[test]
    fn renders_labels_value_and_timestamp() {
        let instance = PrometheusInstance::new()
            .with_label("interface", "wg0")
            .with_label("peer", "abc")
            .with_timestamp(1234)
            .with_value(42_u128);

        assert_eq!(
            instance.render(),
            "{interface=\"wg0\",peer=\"abc\"} 42 1234"
        );
    }

    #[test]
    fn renders_value_without_labels() {
        let instance = PrometheusInstance::new().with_value(42_u128);
        assert_eq!(instance.render(), " 42");
    }

    #[derive(Debug)]
    struct CustomRenderedInstance;

    impl RenderToPrometheus for CustomRenderedInstance {
        fn render(&self) -> String {
            " 7".to_owned()
        }
    }

    #[test]
    fn appends_any_renderable_instance() {
        let mut metric = PrometheusMetric::build()
            .with_name("wireguard_test_total")
            .with_metric_type(MetricType::Counter)
            .with_help("Test metric")
            .build();

        metric.render_and_append_instance(&CustomRenderedInstance);

        assert_eq!(
            metric.render(),
            "# HELP wireguard_test_total Test metric\n# TYPE wireguard_test_total counter\nwireguard_test_total 7\n"
        );
    }

    #[test]
    fn displays_metric_types() {
        assert_eq!(MetricType::Counter.to_string(), "counter");
        assert_eq!(MetricType::Gauge.to_string(), "gauge");
        assert_eq!(MetricType::Histogram.to_string(), "histogram");
        assert_eq!(MetricType::Summary.to_string(), "summary");
    }
}
