//! Application logging.
//!
//! Installs a global `tracing` subscriber that does two things at once:
//!   1. writes structured **JSON** lines to stderr (for the console / headless
//!      mode), and
//!   2. captures every event into an in-memory [`LogBus`] — a bounded ring
//!      buffer plus a broadcast signal — so the in-app server log page can show
//!      a live, filterable/searchable view.
//!
//! [`init`] must be called exactly once, before COSMIC (or the headless server)
//! starts. It returns the [`LogBus`], which is handed to the app.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use serde_json::Map;
use serde_json::Value;
use time::OffsetDateTime;
use time::macros::format_description;
use tokio::sync::broadcast;
use tracing::Event;
use tracing::Level;
use tracing::Subscriber;
use tracing::field::Field;
use tracing::field::Visit;
use tracing::span::Attributes;
use tracing::span::Id;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

/// Max log lines kept in memory for the UI. Older lines are dropped.
const RING_CAPACITY: usize = 5000;

/// Monotonic id source so the UI can reference a specific captured entry
/// (e.g. the one the user clicked) even as the ring buffer scrolls.
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// An enclosing span captured for a log event (root-first order).
#[derive(Clone, Debug)]
pub struct SpanInfo {
    pub name: String,
    pub target: String,
    pub fields: Map<String, Value>,
}

/// One captured log event, already flattened for display.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// Stable, unique id (for click-to-select in the UI).
    pub id: u64,
    /// `HH:MM:SS.mmm`, UTC.
    pub timestamp: String,
    pub level: Level,
    pub target: String,
    pub message: String,
    pub fields: Map<String, Value>,
    /// Enclosing spans at the time of the event, outermost first.
    pub spans: Vec<SpanInfo>,
}

impl LogEntry {
    /// A single-line rendering of the structured fields, for compact display.
    pub fn fields_summary(&self) -> String {
        self.fields
            .iter()
            .map(|(k, v)| format!("{k}={}", render_value(v)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Render a JSON value without the quotes around strings (nicer for the UI).
pub fn render_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Fields captured for a span, stashed in its registry extensions.
struct SpanFields(Map<String, Value>);

/// Shared, cloneable handle to the captured log: a bounded ring buffer plus a
/// broadcast that signals "something changed" (the page re-reads the buffer).
#[derive(Clone)]
pub struct LogBus {
    buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    sender: broadcast::Sender<()>,
}

impl LogBus {
    /// A snapshot of the current buffer (oldest first).
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.buffer.lock().unwrap().iter().cloned().collect()
    }

    /// Subscribe to "buffer changed" signals.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }

    fn push(&self, entry: LogEntry) {
        {
            let mut buf = self.buffer.lock().unwrap();
            if buf.len() == RING_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(entry);
        }
        // Ignore send errors: no active subscribers is fine.
        let _ = self.sender.send(());
    }
}

/// Collects an event's fields, pulling out the special `message` field.
#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Map<String, Value>,
}

impl FieldVisitor {
    fn insert(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = match value {
                Value::String(s) => s,
                other => other.to_string(),
            };
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.insert(field, Value::String(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::String(value.to_string()));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, Value::from(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::from(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, Value::from(value));
    }
}

/// A `tracing` layer that funnels events into a [`LogBus`].
struct BroadcastLayer {
    bus: LogBus,
}

impl<S> Layer<S> for BroadcastLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = FieldVisitor::default();
            attrs.record(&mut visitor);
            span.extensions_mut().insert(SpanFields(visitor.fields));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let meta = event.metadata();

        // Walk the enclosing span scope, outermost (root) first.
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let fields = span
                    .extensions()
                    .get::<SpanFields>()
                    .map(|f| f.0.clone())
                    .unwrap_or_default();
                spans.push(SpanInfo {
                    name: span.name().to_string(),
                    target: span.metadata().target().to_string(),
                    fields,
                });
            }
        }

        self.bus.push(LogEntry {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            timestamp: now_hms(),
            level: *meta.level(),
            target: meta.target().to_string(),
            message: visitor.message,
            fields: visitor.fields,
            spans,
        });
    }
}

fn now_hms() -> String {
    OffsetDateTime::now_utc()
        .format(format_description!(
            "[hour]:[minute]:[second].[subsecond digits:3]"
        ))
        .unwrap_or_default()
}

/// Install the global subscriber (JSON → stderr + in-memory capture) and return
/// the [`LogBus`]. Call once, before the UI or server starts. Honours
/// `RUST_LOG`; defaults to `info` for our crates.
pub fn init() -> LogBus {
    let bus = LogBus {
        buffer: Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAPACITY))),
        sender: broadcast::channel(256).0,
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,read_flow=info,read_flow_core=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json().with_writer(std::io::stderr))
        .with(BroadcastLayer { bus: bus.clone() })
        .init();

    bus
}
