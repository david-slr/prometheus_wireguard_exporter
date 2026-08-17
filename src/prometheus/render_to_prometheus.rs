use std::fmt::Debug;

pub(crate) trait RenderToPrometheus: Debug {
    fn render(&self) -> String;
}
