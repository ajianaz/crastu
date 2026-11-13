//! The schema of `perf.jsonl`. The writer lives on `Recorder::perf`; the host and the desktop app
//! feed it (`FrameStats` for the GUI, `crastu-host-perf` for the host).

use serde::Serialize;

use crastu_redact::ident;

/// How long one kind of job took. The fields are private so that the only way to make one is
/// [`JobTiming::new`], which checks the name.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct JobTiming {
    kind: String,
    ms: u64,
}

impl JobTiming {
    /// `kind` is a job kind such as `transcribe`: letters, digits and `_:-`. Anything else is
    /// recorded as `invalid-name`.
    pub fn new(kind: &str, ms: u64) -> Self {
        JobTiming {
            kind: ident(kind),
            ms,
        }
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn ms(&self) -> u64 {
        self.ms
    }
}

/// One line of `perf.jsonl`, written about once a second. Every field is a number or a job kind,
/// so a sample cannot carry user content. GUI processes fill the frame fields and the host fills
/// the queue and job fields; a missing value is left out of the line.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct PerfSample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_p50_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_p99_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_max_ms: Option<f64>,
    /// Resident memory in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_depth: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<JobTiming>,
}

/// The p50, p99 and max of a window of frame times, in the unit of the input; `None` when it is
/// empty. Uses the nearest-rank method on a sorted copy, and ignores values that are not finite.
pub fn frame_stats(frames: &[f64]) -> Option<(f64, f64, f64)> {
    let mut v: Vec<f64> = frames.iter().copied().filter(|f| f.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let rank = |p: f64| {
        let i = ((p * v.len() as f64).ceil() as usize).clamp(1, v.len());
        v[i - 1]
    };
    Some((rank(0.50), rank(0.99), v[v.len() - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_use_nearest_rank() {
        let frames: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(frame_stats(&frames), Some((50.0, 99.0, 100.0)));
        assert_eq!(frame_stats(&[7.0]), Some((7.0, 7.0, 7.0)));
        assert_eq!(frame_stats(&[]), None);
        assert_eq!(frame_stats(&[f64::NAN]), None);
    }

    #[test]
    fn a_job_kind_is_always_a_plain_name() {
        assert_eq!(
            JobTiming::new("/Users/a/secret.txt", 1).kind(),
            "invalid-name"
        );
        assert_eq!(JobTiming::new("transcribe", 7).ms(), 7);
    }

    #[test]
    fn missing_values_are_left_out() {
        let s = PerfSample {
            queue_depth: Some(3),
            jobs: vec![JobTiming::new("transcribe", 120)],
            ..PerfSample::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"queue_depth": 3, "jobs": [{"kind": "transcribe", "ms": 120}]})
        );
    }
}
