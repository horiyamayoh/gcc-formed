/// Typed error enum for the CLI front-end.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("backend resolution failed: {0}")]
    Backend(String),
    #[error("capture failed: {0}")]
    Capture(#[from] diag_capture_runtime::CaptureError),
    #[error("adapter error: {0}")]
    Adapter(#[from] diag_adapter_gcc::AdapterError),
    #[error("render error: {0}")]
    Render(#[from] diag_render::RenderError),
    #[error("trace error: {0}")]
    Trace(#[from] diag_trace::TraceError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
