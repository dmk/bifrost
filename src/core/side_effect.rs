use async_trait::async_trait;
use std::sync::Arc;
use serde_yaml::Value;

/// Minimal backend identity information provided to side effects.
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub name: String,
    pub server: String,
}

/// Event emitted by the concurrent pool when it obtains a cache hit.
#[derive(Debug, Clone)]
pub struct ConcurrentHitEvent {
    pub backend_index: usize,
    pub response_bytes: Vec<u8>,
    pub backends: Arc<Vec<BackendInfo>>, // index 0 is the primary
}

/// Core trait for side effects (replication, warming, metrics enrichers)
#[async_trait]
pub trait SideEffect: Send + Sync {
    /// Execute the side effect for a concurrent-hit event (best-effort; safe to fail)
    async fn on_concurrent_hit(&self, event: ConcurrentHitEvent);
    fn name(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum SideEffectError {
    #[error("Unknown side effect: {0}")]
    UnknownEffect(String),
}

/// Factory to create a side effect from config
pub fn create_side_effect(config: &crate::config::SideEffectConfig) -> Result<Arc<dyn SideEffect>, SideEffectError> {
    match config.effect_type.as_str() {
        // New configurable replicator
        "replicate_on_hit" => Ok(Arc::new(ReplicateOnHit::from_config(&config.config)) as Arc<dyn SideEffect>),
        // Back-compat alias: defaults to first backend as target
        "replicate_to_primary_on_hit" => Ok(Arc::new(ReplicateOnHit::default_primary()) as Arc<dyn SideEffect>),
        other => Err(SideEffectError::UnknownEffect(other.to_string())),
    }
}

#[derive(Clone, Copy)]
enum ReplicationMethod { Set, Add, Replace }

/// Side effect that replicates a VALUE into a chosen backend when the hit occurs.
pub struct ReplicateOnHit {
    name: String,
    target_backend_names: Vec<String>,
    target_indices: Vec<usize>,
    only_if_from_not_target: bool,
    method: ReplicationMethod,
    exptime_override: Option<u32>,
}

impl ReplicateOnHit {
    fn default_primary() -> Self {
        Self {
            name: "replicate_to_primary_on_hit".to_string(),
            target_backend_names: Vec::new(),
            target_indices: vec![0],
            only_if_from_not_target: true,
            method: ReplicationMethod::Set,
            exptime_override: None,
        }
    }

    fn from_config(cfg: &std::collections::HashMap<String, Value>) -> Self {
        let name = "replicate_on_hit".to_string();
        // Back-compat single fields
        let single_name = cfg.get("target_backend").and_then(Value::as_str).map(|s| s.to_string());
        let single_index = cfg.get("target_index").and_then(Value::as_u64).map(|v| v as usize);
        // New arrays
        let mut target_backend_names: Vec<String> = cfg
            .get("targets")
            .and_then(|v| v.as_sequence())
            .map(|seq| seq.iter().filter_map(|e| e.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        if let Some(n) = single_name { target_backend_names.push(n); }
        let mut target_indices: Vec<usize> = cfg
            .get("target_indices")
            .and_then(|v| v.as_sequence())
            .map(|seq| seq.iter().filter_map(|e| e.as_u64().map(|u| u as usize)).collect())
            .unwrap_or_default();
        if let Some(i) = single_index { target_indices.push(i); }
        let only_if_from_not_target = cfg.get("only_if_from_not_target").and_then(Value::as_bool).unwrap_or(true);
        let method = match cfg.get("method").and_then(Value::as_str).unwrap_or("set").to_lowercase().as_str() {
            "add" => ReplicationMethod::Add,
            "replace" => ReplicationMethod::Replace,
            _ => ReplicationMethod::Set,
        };
        let exptime_override = cfg.get("exptime").and_then(Value::as_u64).map(|v| v as u32);
        Self {
            name,
            target_backend_names,
            target_indices,
            only_if_from_not_target,
            method,
            exptime_override,
        }
    }

    /// Parse an ASCII VALUE response to (key, flags, bytes, data)
    fn parse_value(response_bytes: &[u8]) -> Option<(String, u32, usize, Vec<u8>)> {
        let s = String::from_utf8_lossy(response_bytes);
        let mut lines = s.split("\r\n");
        while let Some(line) = lines.next() {
            if line.starts_with("VALUE ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    break;
                }
                let key = parts[1].to_string();
                let flags = parts[2].parse::<u32>().ok()?;
                let bytes = parts[3].parse::<usize>().ok()?;
                // Next line should be the data
                let data_line = lines.next().unwrap_or("");
                let data = data_line.as_bytes().to_vec();
                return Some((key, flags, bytes, data));
            }
        }
        None
    }
}

#[async_trait]
impl SideEffect for ReplicateOnHit {
    async fn on_concurrent_hit(&self, event: ConcurrentHitEvent) {
        // Resolve target backend indices from names and explicit indices, then dedup and filter self if needed
        let mut targets: Vec<usize> = self.target_indices.clone();
        for name in &self.target_backend_names {
            if let Some((idx, _)) = event.backends.iter().enumerate().find(|(_, b)| &b.name == name) {
                targets.push(idx);
            } else {
                tracing::debug!("side_effect {}: target backend '{}' not found in pool; skipping", self.name(), name);
            }
        }
        // Deduplicate
        targets.sort_unstable();
        targets.dedup();
        if self.only_if_from_not_target {
            targets.retain(|&i| i != event.backend_index);
        }
        if targets.is_empty() {
            return;
        }

        // Parse value from response
        let Some((key, flags, bytes, data)) = Self::parse_value(&event.response_bytes) else {
            tracing::debug!("side_effect {}: could not parse VALUE for replication", self.name());
            return;
        };

        // Build command template with noreply
        let exptime = self.exptime_override.unwrap_or(0);
        let verb = match self.method { ReplicationMethod::Set => "set", ReplicationMethod::Add => "add", ReplicationMethod::Replace => "replace" };
        let mut request = format!("{} {} {} {} {} noreply\r\n", verb, key, flags, exptime, bytes).into_bytes();
        request.extend_from_slice(&data);
        request.extend_from_slice(b"\r\n");

        tracing::debug!("side_effect {}: replicating to {} target(s)", self.name(), targets.len());
        for idx in targets {
            if let Some(target) = event.backends.get(idx) {
                let server = target.server.clone();
                let name = target.name.clone();
                let req = request.clone();
                let effect_name = self.name.clone();
                let key_clone = key.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncWriteExt;
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        tokio::net::TcpStream::connect(&server),
                    )
                    .await
                    {
                        Err(_) => {
                            tracing::debug!("side_effect replicate connect timeout to {} ({})", name, server);
                            return;
                        }
                        Ok(Ok(mut stream)) => {
                            tracing::debug!("side_effect {}: replicating key '{}' ({} bytes) to {}", effect_name, key_clone, bytes, name);
                            if let Err(e) = stream.write_all(&req).await {
                                tracing::debug!("side_effect replicate write failed to {} ({}): {}", name, server, e);
                                return;
                            }
                            if let Err(e) = stream.flush().await {
                                tracing::debug!("side_effect replicate flush failed to {} ({}): {}", name, server, e);
                                return;
                            }
                            tracing::debug!("side_effect replicate sent to {} (noreply)", name);
                        }
                        Ok(Err(e)) => {
                            tracing::debug!("side_effect replicate connect failed to {} ({}): {}", name, server, e);
                        }
                    }
                });
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}