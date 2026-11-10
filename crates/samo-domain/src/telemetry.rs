use samo_blueprint::{MAX_RING_BUFFER_SAMPLES, MAX_SAMPLE_INTERVAL_MS, MIN_SAMPLE_INTERVAL_MS};
use samo_primitives::{Reading, TelemetryPoint};
use std::collections::VecDeque;

pub fn clamp_sample_interval_ms(interval_ms: u64) -> u64 {
    interval_ms.clamp(MIN_SAMPLE_INTERVAL_MS, MAX_SAMPLE_INTERVAL_MS)
}

#[derive(Debug, Clone)]
pub struct TelemetryRingBuffer {
    capacity: usize,
    expected_interval_ms: i64,
    /// A sample later than this after the one before means the sensor stopped.
    gap_after_ms: i64,
    points: VecDeque<TelemetryPoint>,
}

impl TelemetryRingBuffer {
    pub fn new(capacity: usize, expected_interval_ms: u64) -> Self {
        let cap = capacity.clamp(1, MAX_RING_BUFFER_SAMPLES);
        let interval = clamp_sample_interval_ms(expected_interval_ms) as i64;
        Self {
            capacity: cap,
            expected_interval_ms: interval,
            gap_after_ms: interval.saturating_mul(2),
            points: VecDeque::with_capacity(cap),
        }
    }

    /// Waits `gap_after_ms` (at least twice the interval) before calling a late sample a gap, for a platform whose
    /// readings can take seconds when the computer is busy. A late sample is still a measurement.
    pub fn with_gap_after(mut self, gap_after_ms: u64) -> Self {
        let gap_after = i64::try_from(gap_after_ms).unwrap_or(i64::MAX);
        self.gap_after_ms = gap_after.max(self.expected_interval_ms.saturating_mul(2));
        self
    }

    /// R3: Appends a telemetry sample, inserting an explicit `Reading::Stopped` gap point if more than the gap's
    /// time (`2 * expected_interval_ms` unless `with_gap_after` says longer) elapsed since the previous sample.
    pub fn append_sample(&mut self, point: TelemetryPoint) {
        if let Some(last) = self.points.back() {
            let delta = point.timestamp_ms.saturating_sub(last.timestamp_ms);
            if delta > self.gap_after_ms {
                let gap_ts = last.timestamp_ms.saturating_add(self.expected_interval_ms);
                let gap_point = TelemetryPoint {
                    timestamp_ms: gap_ts,
                    generation: last.generation,
                    cpu_overall_pct: Reading::stopped(gap_ts),
                    cpu_perf_cores_pct: Reading::stopped(gap_ts),
                    cpu_eff_cores_pct: Reading::stopped(gap_ts),
                    memory: Reading::stopped(gap_ts),
                    disk_io: Reading::stopped(gap_ts),
                    network: Reading::stopped(gap_ts),
                };
                self.push_bounded(gap_point);
            }
        }
        self.push_bounded(point);
    }

    fn push_bounded(&mut self, point: TelemetryPoint) {
        if self.points.len() >= self.capacity {
            self.points.pop_front();
        }
        self.points.push_back(point);
    }

    pub fn snapshot_vec(&self) -> Vec<TelemetryPoint> {
        self.points.iter().cloned().collect()
    }

    pub fn latest(&self) -> Option<TelemetryPoint> {
        self.points.back().cloned()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_primitives::UnsupportedReason;

    fn dummy_point(ts: i64, gen_id: u64, cpu: Reading<f32>) -> TelemetryPoint {
        TelemetryPoint {
            timestamp_ms: ts,
            generation: gen_id,
            cpu_overall_pct: cpu,
            cpu_perf_cores_pct: Reading::unsupported(UnsupportedReason::SensorUnavailable),
            cpu_eff_cores_pct: Reading::unsupported(UnsupportedReason::SensorUnavailable),
            memory: Reading::unsupported(UnsupportedReason::SensorUnavailable),
            disk_io: Reading::unsupported(UnsupportedReason::SensorUnavailable),
            network: Reading::unsupported(UnsupportedReason::SensorUnavailable),
        }
    }

    #[test]
    fn missed_intervals_insert_stopped_gap_and_never_zero_on_err() {
        let mut buf = TelemetryRingBuffer::new(100, 1000);
        buf.append_sample(dummy_point(1000, 1, Reading::available(25.0, 1000)));
        buf.append_sample(dummy_point(2000, 2, Reading::available(30.0, 2000)));
        // 5-second sleep gap (> 2 * 1000ms)
        buf.append_sample(dummy_point(7000, 3, Reading::available(15.0, 7000)));

        let items = buf.snapshot_vec();
        assert_eq!(items.len(), 4);
        assert_eq!(items[2].cpu_overall_pct, Reading::stopped(3000));
        assert_eq!(items[2].cpu_perf_cores_pct, Reading::stopped(3000));
        // SensorUnavailable stays Unsupported, never 0
        assert_eq!(
            items[0].cpu_perf_cores_pct,
            Reading::unsupported(UnsupportedReason::SensorUnavailable)
        );
    }

    #[test]
    fn a_buffer_can_wait_longer_before_calling_it_a_gap() {
        let cpu = |ts| dummy_point(ts, 1, Reading::available(50.0, ts));
        let mut buf = TelemetryRingBuffer::new(100, 1000).with_gap_after(5_000);
        buf.append_sample(cpu(1_000));
        // 3.3 s later: late, but measured.
        buf.append_sample(cpu(4_300));
        assert_eq!(buf.len(), 2, "a late sample is not a gap");
        // 6 s later: the sensor stopped.
        buf.append_sample(cpu(10_300));
        let items = buf.snapshot_vec();
        assert_eq!(items.len(), 4);
        assert_eq!(items[2].cpu_overall_pct, Reading::stopped(5_300));
    }
}
