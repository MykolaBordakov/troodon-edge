use prometheus::{
    HistogramVec, IntCounterVec, Opts, register_histogram_vec, register_int_counter_vec,
};
use std::sync::LazyLock;

pub static REQ_COUNTER: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        Opts::new(
            "troodon_http_requests_total",
            "Total number of HTTP requests"
        ),
        &["method", "status", "host"]
    )
    .expect("Failed to create metric REQ_COUNTER")
});

pub static REQ_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "troodon_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "status", "host"]
    )
    .expect("Failed to create metric REQ_DURATION")
});
