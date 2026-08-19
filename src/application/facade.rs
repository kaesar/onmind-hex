//! The `services.*` facade exposed to ABCode scripts.
//!
//! Aggregates the output ports into a list of [`HostService`] callbacks that the
//! engine registers as the global `services` object — mirroring hex4w's
//! `ScriptServicesFacade`. Methods whose adapter is not active return an
//! `Unsupported` error (like hex4w's `ObjectProvider` at runtime).

use abcodelib::HostService;
use std::sync::Arc;

use crate::application::ports::{abc_exec_request, AbcPort, CachePort, EmailPort, EventPort, LambdaPort, StorePort};

pub struct FacadeBuilder {
    abc: Option<Arc<dyn AbcPort>>,
    cache: Option<Arc<dyn CachePort>>,
    store: Option<Arc<dyn StorePort>>,
    events: Option<Arc<dyn EventPort>>,
    lambda: Option<Arc<dyn LambdaPort>>,
    email: Option<Arc<dyn EmailPort>>,
}

impl Default for FacadeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FacadeBuilder {
    pub fn new() -> Self {
        Self {
            abc: None,
            cache: None,
            store: None,
            events: None,
            lambda: None,
            email: None,
        }
    }
    pub fn with_abc(mut self, p: Arc<dyn AbcPort>) -> Self {
        self.abc = Some(p);
        self
    }
    pub fn with_cache(mut self, p: Arc<dyn CachePort>) -> Self {
        self.cache = Some(p);
        self
    }
    pub fn with_store(mut self, p: Arc<dyn StorePort>) -> Self {
        self.store = Some(p);
        self
    }
    pub fn with_events(mut self, p: Arc<dyn EventPort>) -> Self {
        self.events = Some(p);
        self
    }
    pub fn with_lambda(mut self, p: Arc<dyn LambdaPort>) -> Self {
        self.lambda = Some(p);
        self
    }
    pub fn with_email(mut self, p: Arc<dyn EmailPort>) -> Self {
        self.email = Some(p);
        self
    }
    pub fn build(self) -> Vec<HostService> {
        let wired = self.wired();
        let mut services: Vec<HostService> = Vec::new();

        if let Some(abc) = self.abc {
            services.push(arc_fn("abcSheet", {
                let abc = Arc::clone(&abc);
                move |args: &[serde_json::Value]| {
                    let show = str_arg(args, 0);
                    let from = str_arg(args, 1);
                    let some = str_arg(args, 2);
                    let resp = abc.sheet(show, from, some).map_err(|e| e.to_string())?;
                    serde_json::to_value(resp).map_err(|e| e.to_string())
                }
            }));
            services.push(arc_fn("abcExec", {
                let abc = Arc::clone(&abc);
                move |args| {
                    let what = args.get(0).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let from = args.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let some = args.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let with = args.get(3).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let puts = args.get(4).cloned().filter(|v| !v.is_null());
                    let req = abc_exec_request(what, from, some, with, puts);
                    let resp = abc.exec(&req).map_err(|e| e.to_string())?;
                    serde_json::to_value(resp).map_err(|e| e.to_string())
                }
            }));
        } else {
            services.push(arc_fn("abcSheet", |_| Err(unsupported())));
            services.push(arc_fn("abcExec", |_| Err(unsupported())));
        }

        if let Some(cache) = self.cache {
            services.push(arc_fn("cacheGet", {
                let cache = Arc::clone(&cache);
                move |args| {
                    let k = str_arg(args, 0).to_string();
                    let v = cache.get(&k).map_err(|e| e.to_string())?;
                    Ok(v.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null))
                }
            }));
            services.push(arc_fn("cacheSet", {
                let cache = Arc::clone(&cache);
                move |args| {
                    let k = str_arg(args, 0).to_string();
                    let v = str_arg(args, 1).to_string();
                    let ttl = u64_arg(args, 2).unwrap_or(300);
                    cache.set(&k, &v, ttl).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!(true))
                }
            }));
            services.push(arc_fn("cacheEvict", {
                let cache = Arc::clone(&cache);
                move |args| {
                    let k = str_arg(args, 0).to_string();
                    cache.evict(&k).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!(true))
                }
            }));
        } else {
            for name in ["cacheGet", "cacheSet", "cacheEvict"] {
                services.push(arc_fn(name, |_| Err(unsupported())));
            }
        }

        // Store (MintStore/S3): saveItem, getItem, listItems, deleteItem
        if let Some(store) = &self.store {
            let store = Arc::clone(store);
            services.push(arc_fn("saveItem", {
                let store = Arc::clone(&store);
                move |args| {
                    let key = str_arg(args, 0).to_string();
                    let content = str_arg(args, 1).as_bytes().to_vec();
                    store.save_item(&key, &content).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!(true))
                }
            }));
            services.push(arc_fn("getItem", {
                let store = Arc::clone(&store);
                move |args| {
                    let key = str_arg(args, 0).to_string();
                    match store.get_item(&key).map_err(|e| e.to_string())? {
                        Some(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned().into()),
                        None => Ok(serde_json::Value::Null),
                    }
                }
            }));
            services.push(arc_fn("listItems", {
                let store = Arc::clone(&store);
                move |args| {
                    let prefix = str_arg(args, 0).to_string();
                    let items = store.list_items(&prefix).map_err(|e| e.to_string())?;
                    serde_json::to_value(items).map_err(|e| e.to_string())
                }
            }));
            services.push(arc_fn("deleteItem", {
                let store = Arc::clone(&store);
                move |args| {
                    let key = str_arg(args, 0).to_string();
                    store.delete_item(&key).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!(true))
                }
            }));
        }

        // Events (SNS/SQS/EventBridge/Kafka/RabbitMQ): publish(topic, key, payload)
        if let Some(events) = &self.events {
            let events = Arc::clone(events);
            services.push(arc_fn("publish", move |args| {
                let topic = str_arg(args, 0).to_string();
                let key = args.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let payload = match args.get(2) {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(v) => serde_json::to_string(v).map_err(|e| e.to_string())?,
                    None => String::new(),
                };
                events.publish(&topic, &key, &payload).map_err(|e| e.to_string())?;
                Ok(serde_json::json!(true))
            }));
        }

        // FaaS (Lambda): invoke, invokeAsync
        if let Some(lambda) = &self.lambda {
            let lambda = Arc::clone(lambda);
            services.push(arc_fn("invoke", {
                let lambda = Arc::clone(&lambda);
                move |args| {
                    let name = str_arg(args, 0).to_string();
                    let payload = json_arg(args, 1).unwrap_or(serde_json::Value::Null);
                    lambda.invoke(&name, &payload).map_err(|e| e.to_string())
                }
            }));
            services.push(arc_fn("invokeAsync", {
                let lambda = Arc::clone(&lambda);
                move |args| {
                    let name = str_arg(args, 0).to_string();
                    let payload = json_arg(args, 1).unwrap_or(serde_json::Value::Null);
                    lambda.invoke_async(&name, &payload).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!(true))
                }
            }));
        }

        // Email (SMTP): sendEmail(to, subject, body)
        if let Some(email) = &self.email {
            let email = Arc::clone(email);
            services.push(arc_fn("sendEmail", move |args| {
                let to = str_arg(args, 0).to_string();
                let subject = str_arg(args, 1).to_string();
                let body = str_arg(args, 2).to_string();
                email.send_email(&to, &subject, &body).map_err(|e| e.to_string())?;
                Ok(serde_json::json!(true))
            }));
        }

        // Output ports not mounted in this build default to Unsupported so the full
        // hex4w `services.*` surface is always present.
        for (name, present) in wired {
            if !present {
                services.push(arc_fn(name, |_| Err(unsupported())));
            }
        }

        services
    }

    fn wired(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("publish", self.events.is_some()),
            ("invoke", self.lambda.is_some()),
            ("invokeAsync", self.lambda.is_some()),
            ("listItems", self.store.is_some()),
            ("saveItem", self.store.is_some()),
            ("getItem", self.store.is_some()),
            ("deleteItem", self.store.is_some()),
            ("sendEmail", self.email.is_some()),
        ]
    }
}

const UNSUPPORTED: &str = "services: adapter not enabled in this build";

fn unsupported() -> String {
    UNSUPPORTED.to_string()
}

fn arc_fn(
    name: &'static str,
    f: impl Fn(&[serde_json::Value]) -> Result<serde_json::Value, String> + Send + Sync + 'static,
) -> HostService {
    HostService {
        name,
        callback: Arc::new(f),
    }
}

fn str_arg(args: &[serde_json::Value], i: usize) -> &str {
    args.get(i)
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

fn u64_arg(args: &[serde_json::Value], i: usize) -> Option<u64> {
    args.get(i).and_then(|v| v.as_u64())
}

fn json_arg(args: &[serde_json::Value], i: usize) -> Option<serde_json::Value> {
    args.get(i).cloned()
}