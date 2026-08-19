//! Infrastructure adapters for the output ports.
//!
//! # feature map
//! - `default`: in-memory cache + XDB HTTP (`abc`); the rest of the
//!   `services.*` surface returns `Unsupported`.
//! - Opt-in adapters mount into the same `services.*` facade as features:
//!   `store` → S3, `events` → SNS, `events-sqs` → SQS, `events-eventbridge` →
//!   EventBridge, `events-kafka` → Kafka (rust-rdkafka), `events-rabbit` →
//!   RabbitMQ (lapin), `lambda` → Lambda, `email` → SMTP, `grpc` → XDB gRPC
//!   server + client, `db` → Role/SQLite.

pub mod cache;
pub mod source;
pub mod engine;
pub mod circuit_breaker;
pub mod cb_decorator;
pub mod cached_abc;
pub mod consumer;
pub use cached_abc::CachedAbcPort;

#[cfg(feature = "abc")]
mod abc_http;

#[cfg(feature = "store")]
mod s3_store;
#[cfg(feature = "store")]
pub use s3_store::S3Store;

#[cfg(feature = "events")]
mod sns_events;
#[cfg(feature = "events")]
pub use sns_events::SnsEventPublisher;

#[cfg(feature = "events-sqs")]
mod sqs_events;
#[cfg(feature = "events-sqs")]
pub use sqs_events::SqsEventPublisher;

#[cfg(feature = "events-eventbridge")]
mod eventbridge_events;
#[cfg(feature = "events-eventbridge")]
pub use eventbridge_events::EventBridgeEventPublisher;

#[cfg(feature = "events-kafka")]
mod kafka_events;
#[cfg(feature = "events-kafka")]
pub use kafka_events::KafkaEventPublisher;

#[cfg(feature = "events-rabbit")]
mod rabbit_events;
#[cfg(feature = "events-rabbit")]
pub use rabbit_events::RabbitEventPublisher;

#[cfg(feature = "lambda")]
mod lambda_client;
#[cfg(feature = "lambda")]
pub use lambda_client::LambdaAdapter;

#[cfg(feature = "email")]
mod smtp_email;
#[cfg(feature = "email")]
pub use smtp_email::SmtpEmailSender;

#[cfg(feature = "grpc")]
pub mod grpc_abc;
#[cfg(feature = "grpc")]
pub use grpc_abc::GrpcAbcClient;

#[cfg(feature = "db")]
mod role;
#[cfg(feature = "db")]
pub use role::SqliteRoleRepository;

pub use cache::InMemoryCache;
#[cfg(feature = "cache")]
pub use cache::RedisCache;

/// Shared synchronous-to-async Tokio runtime for the cloud adapters.
#[cfg(any(
    feature = "store",
    feature = "events",
    feature = "events-sqs",
    feature = "events-eventbridge",
    feature = "events-kafka",
    feature = "events-rabbit",
    feature = "lambda",
    feature = "email"
))]
pub fn cloud_runtime() -> std::sync::Arc<tokio::runtime::Runtime> {
    std::sync::Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio multi-thread runtime"),
    )
}

/// XDB `/abc` HTTP adapter (feature `abc`).
#[cfg(feature = "abc")]
pub use abc_http::XdbHttpAdapter;