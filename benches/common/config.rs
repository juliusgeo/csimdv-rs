use std::env;

#[derive(Debug, Clone)]
pub struct SyntheticConfig {
    pub size_mb: f64,
    pub columns: usize,
    pub quoted_steps: Vec<u32>,
    pub seed: u64,
}

impl Default for SyntheticConfig {
    fn default() -> Self {
        Self {
            size_mb: 5.0,
            columns: 12,
            quoted_steps: vec![10, 20, 30, 40, 50, 60, 70, 80, 90],
            seed: 0x5EED,
        }
    }
}

impl SyntheticConfig {
    /// Reads overrides from the environment, falling back to `Default` for
    /// anything unset or unparsable.
    ///
    /// Recognized vars:
    ///   SIZE_MB        - f64, target size per generated file in MB
    ///   COLUMNS        - usize, number of columns per row
    ///   QUOTED_STEPS   - comma-separated u32 percentages, e.g. "10,30,50,70,90"
    ///   SEED           - u64, RNG seed for reproducible generation
    pub fn from_env() -> Self {
        let default = Self::default();
        Self {
            size_mb: env_f64("SIZE_MB", default.size_mb),
            columns: env_usize("COLUMNS", default.columns),
            quoted_steps: env_u32_list("QUOTED_STEPS", default.quoted_steps),
            seed: env_u64("SEED", default.seed),
        }
    }
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

fn env_u32_list(key: &str, default: Vec<u32>) -> Vec<u32> {
    match env::var(key) {
        Ok(s) if !s.trim().is_empty() => {
            let parsed: Vec<u32> = s
                .split(',')
                .filter_map(|p| p.trim().parse().ok())
                .collect();
            if parsed.is_empty() {
                default
            } else {
                parsed
            }
        }
        _ => default,
    }
}