//! Logs and traces. Everything the hub observes is a `tracing` span or event;
//! this module decides where they go.
//!
//! - stdout: JSON lines (`HUB_LOG_FORMAT=pretty` for humans).
//! - `HUB_LOG_DIR`: the same JSON, in files rotated daily, keeping
//!   `HUB_LOG_KEEP` days (default 14).
//! - OTLP (built with `--features otel`, and `OTEL_EXPORTER_OTLP_ENDPOINT`
//!   set): spans exported to a collector (Jaeger, Tempo, Honeycomb…). Standard
//!   `OTEL_*` variables configure it.
//!
//! `RUST_LOG` filters as usual; the default is `info` for the hub's crates and
//! `warn` for libraries.
//!
//! The browser sends a W3C `traceparent` header with each call. Without OTel
//! the request span records it as `traceparent`, so one page view's calls can
//! be grouped in the logs; with OTel the span joins that trace.

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// Keep alive for the life of the process: dropping it flushes the file
/// writer and the OTLP exporter.
pub struct Guard {
    _file: Option<WorkerGuard>,
    #[cfg(feature = "otel")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(p) = self.provider.take() {
            let _ = p.shutdown();
        }
    }
}

pub fn init() -> Guard {
    let filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("warn,hub_server=info,hub_ingest=info,hub_store=info,hub_proof=info,hub_biel=info,tower_http=info")
        })
    };
    let pretty = std::env::var("HUB_LOG_FORMAT").is_ok_and(|v| v == "pretty");
    let stdout = if pretty {
        fmt::layer().with_target(false).boxed()
    } else {
        fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .boxed()
    };

    let (file, file_guard) = match std::env::var("HUB_LOG_DIR") {
        Ok(dir) => {
            let keep = std::env::var("HUB_LOG_KEEP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(14);
            let appender = tracing_appender::rolling::Builder::new()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("hub")
                .filename_suffix("jsonl")
                .max_log_files(keep)
                .build(&dir)
                .expect("HUB_LOG_DIR must be a writable directory");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .with_writer(writer)
                .with_filter(filter())
                .boxed();
            (Some(layer), Some(guard))
        }
        Err(_) => (None, None),
    };

    #[cfg(feature = "otel")]
    let (otel, provider) = otel_layer();
    #[cfg(not(feature = "otel"))]
    let otel: Option<Box<dyn Layer<_> + Send + Sync>> = None;

    tracing_subscriber::registry()
        .with(stdout.with_filter(filter()))
        .with(file)
        .with(otel)
        .init();

    Guard {
        _file: file_guard,
        #[cfg(feature = "otel")]
        provider,
    }
}

#[cfg(feature = "otel")]
fn otel_layer<S>() -> (
    Option<Box<dyn Layer<S> + Send + Sync>>,
    Option<opentelemetry_sdk::trace::SdkTracerProvider>,
)
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    use opentelemetry::trace::TracerProvider;
    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_err() {
        return (None, None);
    }
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .expect("OTLP exporter");
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("translation-hub")
                .build(),
        )
        .build();
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );
    let tracer = provider.tracer("hub-server");
    let layer = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(EnvFilter::new("info"))
        .boxed();
    (Some(layer), Some(provider))
}

/// Joins the request span to the browser's trace, when OTel is on.
#[cfg(feature = "otel")]
pub fn link_parent(span: &tracing::Span, headers: &axum::http::HeaderMap) {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    struct Headers<'a>(&'a axum::http::HeaderMap);
    impl opentelemetry::propagation::Extractor for Headers<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|v| v.to_str().ok())
        }
        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(|k| k.as_str()).collect()
        }
    }
    let parent = opentelemetry::global::get_text_map_propagator(|p| p.extract(&Headers(headers)));
    let _ = span.set_parent(parent);
}

#[cfg(not(feature = "otel"))]
pub fn link_parent(_span: &tracing::Span, _headers: &axum::http::HeaderMap) {}
