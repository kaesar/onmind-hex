//! RabbitMQ script-command consumer (feature `events-rabbit`, lapin),
//! mirroring hex4w `RabbitMQEventConsumerAdapter`. Declares its own queue and
//! binds it to the `hex.events` exchange with the command routing key.
//!
//! Env:
//! - `HEX_RABBIT_URL`       (default `amqp://guest:guest@localhost:5672`)
//! - `HEX_RABBIT_EXCHANGE`  (default `hex.events`)
//! - `HEX_RABBIT_CONSUMER_QUEUE` (default `hex.script.commands`)

use std::sync::Arc;

use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, ExchangeDeclareOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties, ExchangeKind};

use crate::application::ScriptCommandUseCase;
use super::parse_command;

pub fn start(use_case: &Arc<ScriptCommandUseCase>, topic: &str) -> bool {
    let url = std::env::var("HEX_RABBIT_URL")
        .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672".into());
    let exchange =
        std::env::var("HEX_RABBIT_EXCHANGE").unwrap_or_else(|_| "hex.events".into());
    let queue = std::env::var("HEX_RABBIT_CONSUMER_QUEUE")
        .unwrap_or_else(|_| "hex.script.commands".into());
    let use_case = Arc::clone(use_case);
    let topic = topic.to_string();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("rabbit consumer runtime");
        rt.block_on(async move {
            let conn = match Connection::connect(&url, ConnectionProperties::default()).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[hex] rabbit consumer connect: {e}");
                    return;
                }
            };
            let channel = match conn.create_channel().await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[hex] rabbit consumer channel: {e}");
                    return;
                }
            };
            let _ = channel
                .exchange_declare(
                    &exchange,
                    ExchangeKind::Topic,
                    ExchangeDeclareOptions::default(),
                    FieldTable::default(),
                )
                .await;
            let _ = channel
                .queue_declare(&queue, QueueDeclareOptions::default(), FieldTable::default())
                .await;
            let _ = channel
                .queue_bind(
                    &queue,
                    &exchange,
                    &topic,
                    QueueBindOptions::default(),
                    FieldTable::default(),
                )
                .await;

            let mut consumer = match channel
                .basic_consume(
                    &queue,
                    "hex-script-runner",
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[hex] rabbit consumer basic_consume: {e}");
                    return;
                }
            };

            while let Some(Some(delivery)) = (consumer.next().await).map(std::result::Result::ok) {
                if let Some(cmd) = parse_command(&delivery.data) {
                    let _ = use_case.handle(&cmd);
                } else {
                    eprintln!("[hex] rabbit: dropping unparseable message");
                }
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
        });
    });

    true
}