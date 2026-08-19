//! SQS script-command consumer (feature `events-sqs`), mirroring hex4w
//! `SqsEventConsumerAdapter`: long-poll, process, delete.
//!
//! Env: `HEX_SQS_QUEUE_URL` (required).

use std::sync::Arc;
use std::time::Duration;

use aws_config::BehaviorVersion;

use crate::application::ScriptCommandUseCase;
use super::parse_command;

pub fn start(use_case: &Arc<ScriptCommandUseCase>, _topic: &str) -> bool {
    let Ok(queue_url) = std::env::var("HEX_SQS_QUEUE_URL") else {
        eprintln!("[hex] sqs consumer requires HEX_SQS_QUEUE_URL");
        return false;
    };
    let rt = std::sync::Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("sqs consumer runtime"),
    );
    let cfg = rt.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
    let client = aws_sdk_sqs::Client::new(&cfg);
    let use_case = Arc::clone(use_case);

    std::thread::spawn(move || loop {
        match rt.block_on(
            client
                .receive_message()
                .queue_url(&queue_url)
                .max_number_of_messages(10)
                .wait_time_seconds(20)
                .visibility_timeout(30)
                .send(),
        ) {
            Ok(resp) => {
                for msg in resp.messages() {
                    let body = msg.body().unwrap_or_default();
                    match parse_command(body.as_bytes()) {
                        Some(cmd) => {
                            let _ = use_case.handle(&cmd);
                        }
                        None => eprintln!(
                            "[hex] sqs: dropping unparseable message id={}",
                            msg.message_id().unwrap_or("-")
                        ),
                    }
                    if let Some(receipt) = msg.receipt_handle() {
                        let _ = rt.block_on(
                            client
                                .delete_message()
                                .queue_url(&queue_url)
                                .receipt_handle(receipt)
                                .send(),
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("[hex] sqs receive: {e}");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    });

    true
}