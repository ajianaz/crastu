use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

struct Shared {
    buf: Mutex<VecDeque<f32>>,
    capacity: usize,
    dropped: AtomicU64,
}

/// The capture side of the ring: `push` never blocks, so it is safe in an audio callback.
#[derive(Debug)]
pub struct Producer(Arc<Shared>);

/// The consumer side of the ring.
#[derive(Debug)]
pub struct Consumer(Arc<Shared>);

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ring")
            .field("capacity", &self.capacity)
            .field("dropped_frames", &self.dropped.load(Ordering::Relaxed))
            .finish()
    }
}

/// A bounded single-producer, single-consumer buffer of mono samples (one sample is one frame).
/// When the consumer falls behind, the newest samples are dropped and counted in
/// [`Producer::dropped_frames`]; the producer never waits for the consumer.
pub fn ring_buffer(capacity: usize) -> (Producer, Consumer) {
    let shared = Arc::new(Shared {
        buf: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
        dropped: AtomicU64::new(0),
    });
    (Producer(shared.clone()), Consumer(shared))
}

impl Producer {
    /// Adds samples. Returns how many were stored; the rest (ring full, or the consumer holding
    /// the lock at that moment) are dropped and counted.
    pub fn push(&self, samples: &[f32]) -> usize {
        let stored = match self.0.buf.try_lock() {
            Ok(mut q) => Self::store(&mut q, self.0.capacity, samples),
            Err(TryLockError::Poisoned(p)) => {
                Self::store(&mut p.into_inner(), self.0.capacity, samples)
            }
            Err(TryLockError::WouldBlock) => 0,
        };
        let lost = samples.len() - stored;
        if lost > 0 {
            self.0.dropped.fetch_add(lost as u64, Ordering::Relaxed);
        }
        stored
    }

    fn store(q: &mut VecDeque<f32>, capacity: usize, samples: &[f32]) -> usize {
        let room = capacity.saturating_sub(q.len());
        let n = room.min(samples.len());
        q.extend(&samples[..n]);
        n
    }

    /// Samples dropped so far because the ring was full or busy.
    pub fn dropped_frames(&self) -> u64 {
        self.0.dropped.load(Ordering::Relaxed)
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
        let (p, c) = ring_buffer(8);
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
        let (p, c) = ring_buffer(4);
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
    fn a_busy_consumer_never_blocks_the_producer() {
        let (p, c) = ring_buffer(8);
        let guard = c.0.buf.lock().unwrap();
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 0);
        assert_eq!(p.dropped_frames(), 3);
        drop(guard);
        assert_eq!(p.push(&[1.0]), 1);
    }

    #[test]
    fn works_across_threads() {
        let (p, c) = ring_buffer(1 << 16);
        let t = std::thread::spawn(move || {
            for i in 0..1000 {
                p.push(&[i as f32]);
            }
            p
        });
        let p = t.join().unwrap();
        let mut out = vec![0.0; 2000];
        let n = c.pop(&mut out);
        assert_eq!(n as u64 + p.dropped_frames(), 1000);
    }
}
