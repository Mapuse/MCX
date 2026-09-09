use crate::core::arch::Architecture;
use crate::core::constants;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SystemProfile {
    pub cpu_count: usize,
    pub cpu_model: String,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub kernel: String,
    pub os_name: String,
    pub is_musl: bool,
    pub has_sandbox: bool,
    pub architecture: Architecture,
}

#[derive(Debug, Clone)]
pub struct CalibratedParams {
    pub thread_pool_size: usize,
    pub concurrent_downloads: usize,
    pub zstd_level: i32,
    pub io_parallelism: usize,
    pub network_latency_adaptive: bool,
    pub latency_threshold_ms: u64,
    pub bandwidth_threshold_kbps: u64,
}

impl Default for CalibratedParams {
    fn default() -> Self {
        let cpus = num_cpus::get();
        Self {
            thread_pool_size: cpus,
            concurrent_downloads: cpus.min(constants::DEFAULT_MAX_CONCURRENT_DOWNLOADS),
            zstd_level: constants::DEFAULT_ZSTD_LEVEL,
            io_parallelism: cpus.max(2),
            network_latency_adaptive: true,
            latency_threshold_ms: constants::DEFAULT_LATENCY_THRESHOLD_MS,
            bandwidth_threshold_kbps: constants::DEFAULT_BANDWIDTH_THRESHOLD_KBPS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkProfile {
    pub latency_ms: f64,
    pub bandwidth_kbps: u64,
    pub rtt_jitter_ms: f64,
    pub fallback_available: bool,
}

#[derive(Debug, Clone)]
pub struct DecisionMatrix {
    pub verdict: HeuristicVerdict,
    pub confidence: f64,
    pub rationale: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeuristicVerdict {
    UseParallel,
    UseSequential,
    UseFallbackRepo,
    UsePrimaryRepo,
    ScaleUpThreadPool,
    ScaleDownThreadPool,
    EnableDelta,
    DisableDelta,
    SafeInstall,
    DeepCleanRequired,
}

impl SystemProfile {
    pub fn probe() -> Self {
        let cpu_count = num_cpus::get();
        let cpu_model = Self::read_cpu_model();
        let (total_ram_mb, available_ram_mb) = Self::read_memory();
        let kernel = Self::read_kernel();
        let os_name = Self::read_os();
        let is_musl = Self::detect_musl();
        let has_sandbox = Self::detect_sandbox();
        let architecture = Architecture::host();
        Self {
            cpu_count,
            cpu_model,
            total_ram_mb,
            available_ram_mb,
            kernel,
            os_name,
            is_musl,
            has_sandbox,
            architecture,
        }
    }

    fn read_cpu_model() -> String {
        fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("model name"))
                    .and_then(|l| l.split(':').nth(1))
                    .map(|s| s.trim().to_string())
            })
            .unwrap_or_else(|| "unknown".into())
    }

    fn read_memory() -> (u64, u64) {
        let info = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let total = info
            .lines()
            .find(|l| l.starts_with("MemTotal:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|kb| kb / 1024)
            .unwrap_or(0);
        let available = info
            .lines()
            .find(|l| l.starts_with("MemAvailable:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|kb| kb / 1024)
            .unwrap_or(0);
        (total, available)
    }

    fn read_kernel() -> String {
        let content = fs::read_to_string("/proc/version").unwrap_or_default();
        content
            .split_whitespace()
            .nth(2)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".into())
    }

    fn read_os() -> String {
        fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .and_then(|l| l.split('=').nth(1))
                    .map(|s| s.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| {
                fs::read_to_string("/etc/lsb-release").unwrap_or_else(|_| "unknown".into())
            })
    }

    fn detect_musl() -> bool {
        fs::read_to_string("/proc/version")
            .map(|s| s.to_lowercase().contains("musl"))
            .unwrap_or(false)
    }

    fn detect_sandbox() -> bool {
        Path::new("/.dockerenv").exists()
            || fs::read_to_string("/proc/1/cgroup")
                .map(|s| s.contains("docker") || s.contains("pod") || s.contains("lxc"))
                .unwrap_or(false)
    }
}

pub struct NetworkProber;

impl NetworkProber {
    pub async fn probe(target: &str, timeout: Duration) -> NetworkProfile {
        use futures_util::future::join_all;

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .no_proxy()
            .build()
            .ok();
        // All samples run concurrently: total wall time is one round trip
        // instead of PROBE_SAMPLES serialized requests.
        let futures = (0..constants::PROBE_SAMPLES).map(|_| async {
            let Some(ref c) = client else {
                return timeout.as_secs_f64() * 1000.0;
            };
            let t0 = Instant::now();
            match c.head(target).send().await {
                Ok(_) => t0.elapsed().as_secs_f64() * 1000.0,
                Err(_) => timeout.as_secs_f64() * 1000.0,
            }
        });
        let samples: Vec<f64> = join_all(futures).await;
        let latency_ms = if samples.is_empty() {
            constants::DEFAULT_FALLBACK_LATENCY
        } else {
            samples.iter().sum::<f64>() / samples.len() as f64
        };
        let rtt_jitter_ms = if samples.len() < 2 {
            0.0
        } else {
            let mean = latency_ms;
            samples.iter().map(|s| (s - mean).abs()).sum::<f64>() / samples.len() as f64
        };
        NetworkProfile {
            latency_ms,
            bandwidth_kbps: constants::DEFAULT_BANDWIDTH_KBPS,
            rtt_jitter_ms,
            fallback_available: latency_ms < constants::FALLBACK_LATENCY_THRESHOLD,
        }
    }
}

pub struct DecisionEngine;

impl DecisionEngine {
    pub fn evaluate_thread_strategy(
        sys: &SystemProfile,
        net: &NetworkProfile,
        params: &CalibratedParams,
    ) -> Vec<DecisionMatrix> {
        let mut decisions = Vec::new();

        let latency_ratio = net.latency_ms / params.latency_threshold_ms as f64;

        decisions.push({
            if sys.cpu_count >= constants::CPU_THRESHOLD_MEDIUM
                && sys.available_ram_mb >= constants::RAM_THRESHOLD_HIGH_MB
                && latency_ratio < constants::LATENCY_RATIO_PARALLEL
            {
                DecisionMatrix {
                    verdict: HeuristicVerdict::UseParallel,
                    confidence: constants::DECISION_CONFIDENCE_HIGH,
                    rationale: "CPU cores >= 8, RAM >= 2GB, low network latency",
                }
            } else if sys.cpu_count >= constants::CPU_THRESHOLD_LOW
                && latency_ratio < constants::LATENCY_RATIO_SEQUENTIAL
            {
                DecisionMatrix {
                    verdict: HeuristicVerdict::UseParallel,
                    confidence: constants::DECISION_CONFIDENCE_MEDIUM,
                    rationale: "Adequate CPU with low latency",
                }
            } else {
                DecisionMatrix {
                    verdict: HeuristicVerdict::UseSequential,
                    confidence: constants::DECISION_CONFIDENCE_LOW,
                    rationale: "Constrained resources or high latency",
                }
            }
        });

        decisions.push({
            if net.latency_ms
                > params.latency_threshold_ms as f64 * constants::LATENCY_SPIKE_CRITICAL
                || net.bandwidth_kbps < params.bandwidth_threshold_kbps / 2
            {
                DecisionMatrix {
                    verdict: HeuristicVerdict::UseFallbackRepo,
                    confidence: constants::DECISION_CONFIDENCE_MEDIUM,
                    rationale: "Primary repo degradation detected",
                }
            } else {
                DecisionMatrix {
                    verdict: HeuristicVerdict::UsePrimaryRepo,
                    confidence: constants::DECISION_CONFIDENCE_HIGH,
                    rationale: "Primary repo within acceptable thresholds",
                }
            }
        });

        decisions.push({
            if net.rtt_jitter_ms > constants::JITTER_THRESHOLD_MS && sys.cpu_count >= constants::CPU_THRESHOLD_LOW {
                DecisionMatrix {
                    verdict: HeuristicVerdict::ScaleUpThreadPool,
                    confidence: constants::DECISION_CONFIDENCE_LOW,
                    rationale: "High jitter suggests variable network; scale threads to mask latency",
                }
            } else if sys.available_ram_mb < constants::RAM_THRESHOLD_LOW_MB {
                DecisionMatrix {
                    verdict: HeuristicVerdict::ScaleDownThreadPool,
                    confidence: constants::DECISION_CONFIDENCE_MEDIUM,
                    rationale: "Limited RAM; reduce thread pool to avoid OOM",
                }
            } else {
                DecisionMatrix {
                    verdict: HeuristicVerdict::UseParallel,
                    confidence: constants::DECISION_CONFIDENCE_LOW,
                    rationale: "Stable conditions; maintain current pool",
                }
            }
        });

        decisions
    }

    pub fn should_use_parallel(decisions: &[DecisionMatrix]) -> bool {
        decisions
            .iter()
            .any(|d| d.verdict == HeuristicVerdict::UseParallel)
            && !decisions
                .iter()
                .any(|d| d.verdict == HeuristicVerdict::UseSequential)
    }
}
