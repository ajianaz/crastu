use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

struct Shared {
    buf: Mutex<VecDeque<f32>>,
    capacity: usize,
    dropped: AtomicU64,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ring")
            .field("capacity", &self.capacity)
            .field("dropped_frames", &self.dropped.load(Ordering::Relaxed))
            .finish()
    }
}

/// The capture side of the ring: `push` never blocks, so it is safe in an audio callback.
#[derive(Debug)]
pub struct Producer {
    shared: Arc<Shared>,
    /// Samples that arrived while the consumer held the lock; they go in, in order, with the next
    /// push. Preallocated, so the callback never allocates.
    pending: Vec<f32>,
    pending_cap: usize,
}

/// The consumer side of the ring.
#[derive(Debug)]
pub struct Consumer(Arc<Shared>);

/// A bounded single-producer, single-consumer buffer of mono samples (one sample is one frame).
/// The producer never waits for the consumer. When the consumer holds the lock at that moment,
/// up to a quarter of the capacity waits in a producer-side slot and goes in with the next push,
/// so a poll that overlaps a callback loses nothing. Only when the ring is full, or that slot
/// overflows, are samples dropped (the newest first) and counted in
/// [`Producer::dropped_frames`].
pub fn ring_buffer(capacity: usize) -> (Producer, Consumer) {
    let shared = Arc::new(Shared {
        buf: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
        dropped: AtomicU64::new(0),
    });
    let pending_cap = capacity / 4;
    (
        Producer {
            shared: shared.clone(),
            pending: Vec::with_capacity(pending_cap),
            pending_cap,
        },
        Consumer(shared),
    )
}

fn store(
    q: &mut VecDeque<f32>,
    pending: &mut Vec<f32>,
    capacity: usize,
    samples: &[f32],
    lost: &mut usize,
) -> usize {
    let room = capacity.saturating_sub(q.len());
    let from_pending = room.min(pending.len());
    q.extend(&pending[..from_pending]);
    *lost += pending.len() - from_pending;
    pending.clear();
    let n = (room - from_pending).min(samples.len());
    q.extend(&samples[..n]);
    *lost += samples.len() - n;
    n
}

impl Producer {
    /// Adds samples; returns how many of them were stored in the ring or the waiting slot. The
    /// rest are dropped and counted.
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let mut lost = 0;
        let cap = self.shared.capacity;
        let kept = match self.shared.buf.try_lock() {
            Ok(mut q) => store(&mut q, &mut self.pending, cap, samples, &mut lost),
            Err(TryLockError::Poisoned(p)) => store(
                &mut p.into_inner(),
                &mut self.pending,
                cap,
                samples,
                &mut lost,
            ),
            Err(TryLockError::WouldBlock) => {
                let room = self.pending_cap.saturating_sub(self.pending.len());
                let n = room.min(samples.len());
                self.pending.extend_from_slice(&samples[..n]);
                lost = samples.len() - n;
                n
            }
        };
        if lost > 0 {
            self.shared
                .dropped
                .fetch_add(lost as u64, Ordering::Relaxed);
        }
        kept
    }

    /// Samples waiting in the producer-side slot for the next push.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Samples dropped so far because the ring was full or the waiting slot overflowed.
    pub fn dropped_frames(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

impl Drop for Producer {
    /// Samples still waiting in the slot when the producer goes away (the stream stopped) were
    /// never stored: they count as dropped.
    fn drop(&mut self) {
        if !self.pending.is_empty() {
            self.shared
                .dropped
                .fetch_add(self.pending.len() as u64, Ordering::Relaxed);
        }
    }
}

impl Consumer {
    /// Moves up to `out.len()` samples, oldest first, into `out`; returns how many.
    pub fn pop(&self, out: &mut [f32]) -> usize {
        let mut q = self.0.buf.lock().unwrap_or_else(|p| p.into_inner());
        let n = out.len().min(q.len());
        for slot in out.iter_mut().take(n) {
            // `n <= q.len()`, so the queue is not empty here.
            *slot = q.pop_front().unwrap_or(0.0);
        }
        n
    }

    /// Samples waiting.
    pub fn len(&self) -> usize {
        self.0.buf.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dropped_frames(&self) -> u64 {
        self.0.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_order() {
        let (mut p, c) = ring_buffer(8);
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0; 2];
        assert_eq!(c.pop(&mut out), 2);
        assert_eq!(out, [1.0, 2.0]);
        assert_eq!(p.push(&[4.0]), 1);
        let mut out = [0.0; 4];
        assert_eq!(c.pop(&mut out), 2);
        assert_eq!(&out[..2], &[3.0, 4.0]);
        assert!(c.is_empty());
    }

    #[test]
    fn overflow_drops_the_newest_and_counts_them() {
        let (mut p, c) = ring_buffer(4);
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 3);
        assert_eq!(p.push(&[4.0, 5.0, 6.0]), 1);
        assert_eq!(p.dropped_frames(), 2);
        assert_eq!(c.dropped_frames(), 2);
        let mut out = [0.0; 8];
        assert_eq!(c.pop(&mut out), 4);
        assert_eq!(&out[..4], &[1.0, 2.0, 3.0, 4.0]);
        // Room again after the consumer caught up.
        assert_eq!(p.push(&[7.0]), 1);
        assert_eq!(p.dropped_frames(), 2);
    }

    #[test]
    fn a_busy_consumer_never_blocks_and_loses_nothing_within_the_slot() {
        let (mut p, c) = ring_buffer(16); // slot of 4
        let guard = c.0.buf.lock().unwrap();
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 3);
        assert_eq!(p.pending(), 3);
        assert_eq!(p.dropped_frames(), 0);
        drop(guard);
        assert_eq!(p.push(&[4.0]), 1);
        assert_eq!(p.pending(), 0);
        let mut out = [0.0; 8];
        assert_eq!(c.pop(&mut out), 4);
        assert_eq!(&out[..4], &[1.0, 2.0, 3.0, 4.0]); // in order
    }

    #[test]
    fn the_waiting_slot_is_bounded_and_overflow_is_counted() {
        let (mut p, c) = ring_buffer(8); // slot of 2
        let guard = c.0.buf.lock().unwrap();
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 2);
        assert_eq!(p.push(&[4.0]), 0);
        assert_eq!(p.dropped_frames(), 2);
        drop(guard);
        assert_eq!(p.push(&[9.0]), 1);
        let mut out = [0.0; 8];
        assert_eq!(c.pop(&mut out), 3);
        assert_eq!(&out[..3], &[1.0, 2.0, 9.0]);
    }

    #[test]
    fn samples_waiting_when_the_producer_goes_away_are_counted() {
        let (mut p, c) = ring_buffer(16);
        let guard = c.0.buf.lock().unwrap();
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 3);
        drop(guard);
        drop(p);
        assert_eq!(c.dropped_frames(), 3);
        // A producer that flushed everything adds nothing.
        let (mut p, c) = ring_buffer(16);
        p.push(&[1.0]);
        drop(p);
        assert_eq!(c.dropped_frames(), 0);
    }

    #[test]
    fn a_full_ring_drops_the_waiting_samples_too() {
        let (mut p, c) = ring_buffer(4); // slot of 1
        p.push(&[1.0, 2.0, 3.0, 4.0]);
        let guard = c.0.buf.lock().unwrap();
        assert_eq!(p.push(&[5.0, 6.0]), 1); // 5 waits, 6 is lost
        drop(guard);
        assert_eq!(p.push(&[7.0]), 0); // the ring is still full: 5 and 7 are lost
        assert_eq!(p.dropped_frames(), 3);
        assert_eq!(c.len(), 4);
    }

    #[test]
    fn a_polling_consumer_loses_no_audio_in_a_stress_run() {
        let (mut p, c) = ring_buffer(1 << 14);
        let t = std::thread::spawn(move || {
            for i in 0..20_000u32 {
                p.push(&[i as f32]);
            }
            p
        });
        let mut got = Vec::new();
        let mut out = [0.0f32; 64];
        while !t.is_finished() {
            let n = c.pop(&mut out);
            got.extend_from_slice(&out[..n]);
        }
        let mut p = t.join().unwrap();
        p.push(&[]); // flush the waiting slot
        loop {
            let n = c.pop(&mut out);
            if n == 0 {
                break;
            }
            got.extend_from_slice(&out[..n]);
        }
        assert_eq!(got.len() as u64 + p.dropped_frames(), 20_000);
        assert!(got.windows(2).all(|w| w[0] < w[1]), "order kept");
    }
}
