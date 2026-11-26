//! The queue of titles waiting for metadata, and the rate of UI updates.

use std::collections::{HashSet, VecDeque};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::state::lock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Job {
    pub title_id: i64,
    /// Which run of the queue the job was taken from.
    epoch: u64,
}

#[derive(Default)]
struct Inner {
    pending: VecDeque<i64>,
    queued: HashSet<i64>,
    running: HashSet<i64>,
    /// Asked for again while running.
    again: HashSet<i64>,
    epoch: u64,
}

impl Inner {
    fn push(&mut self, title_id: i64) {
        if self.running.contains(&title_id) {
            self.again.insert(title_id);
        } else if self.queued.insert(title_id) {
            self.pending.push_back(title_id);
        }
    }

    fn drop_pending(&mut self) {
        self.pending.clear();
        self.queued.clear();
        self.again.clear();
    }

    fn take(&mut self) -> Option<Job> {
        let title_id = self.pending.pop_front()?;
        self.queued.remove(&title_id);
        self.running.insert(title_id);
        Some(Job {
            title_id,
            epoch: self.epoch,
        })
    }
}

/// A title is never queued twice, nor worked on by two workers at once.
#[derive(Default)]
pub struct JobQueue {
    inner: Mutex<Inner>,
    ready: Condvar,
}

impl JobQueue {
    pub fn push(&self, title_ids: impl IntoIterator<Item = i64>) {
        let mut inner = lock(&self.inner);
        for title_id in title_ids {
            inner.push(title_id);
        }
        drop(inner);
        self.ready.notify_all();
    }

    /// Blocks until there is work.
    pub fn next(&self) -> Job {
        let mut inner = lock(&self.inner);
        loop {
            if let Some(job) = inner.take() {
                return job;
            }
            inner = self
                .ready
                .wait(inner)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Must be called once for every job taken. True if no work is left.
    pub fn finish(&self, job: Job) -> bool {
        let mut inner = lock(&self.inner);
        inner.running.remove(&job.title_id);
        if inner.again.remove(&job.title_id) {
            inner.push(job.title_id);
            self.ready.notify_one();
        }
        inner.running.is_empty() && inner.pending.is_empty()
    }

    /// Drops the waiting work because `job` showed that none of it can succeed.
    /// Does nothing if the queue has been reset since the job was taken.
    pub fn abort(&self, job: Job) {
        let mut inner = lock(&self.inner);
        if job.epoch == inner.epoch {
            inner.drop_pending();
        }
    }

    /// Drops the waiting work, and ignores any later `abort` from running jobs.
    pub fn reset(&self) {
        let mut inner = lock(&self.inner);
        inner.drop_pending();
        inner.epoch += 1;
    }
}

const UPDATE_INTERVAL: Duration = Duration::from_secs(1);

/// Decides when finished jobs are worth telling the UI about.
#[derive(Default)]
pub struct Throttle {
    last: Option<Instant>,
    dirty: bool,
}

impl Throttle {
    /// True if the UI should reload now. The last job always flushes.
    pub fn record(&mut self, changed: bool, idle: bool, now: Instant) -> bool {
        self.dirty |= changed;
        let due = idle
            || self
                .last
                .is_none_or(|last| now.duration_since(last) >= UPDATE_INTERVAL);
        let emit = self.dirty && due;
        if emit {
            self.dirty = false;
            self.last = Some(now);
        }
        emit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(queue: &JobQueue) -> Option<Job> {
        lock(&queue.inner).take()
    }

    #[test]
    fn titles_are_queued_once() {
        let queue = JobQueue::default();
        queue.push([1, 2, 1]);
        assert_eq!(queue.next().title_id, 1);
        assert_eq!(queue.next().title_id, 2);
        assert_eq!(take(&queue), None);
    }

    #[test]
    fn a_running_title_is_repeated_afterwards() {
        let queue = JobQueue::default();
        queue.push([1]);
        let job = queue.next();
        queue.push([1]);
        assert_eq!(take(&queue), None);
        assert!(!queue.finish(job));
        let repeat = queue.next();
        assert_eq!(repeat.title_id, 1);
        assert!(queue.finish(repeat));
    }

    #[test]
    fn aborting_drops_the_waiting_work() {
        let queue = JobQueue::default();
        queue.push([1, 2]);
        let job = queue.next();
        queue.push([1]);
        queue.abort(job);
        assert!(queue.finish(job));
        assert_eq!(take(&queue), None);
    }

    #[test]
    fn an_old_job_cannot_abort_after_a_reset() {
        let queue = JobQueue::default();
        queue.push([1]);
        let old = queue.next();
        queue.reset();
        queue.push([1, 2]);
        queue.abort(old);
        assert!(!queue.finish(old));
        assert_eq!(queue.next().title_id, 2);
        assert_eq!(queue.next().title_id, 1);
    }

    #[test]
    fn updates_are_throttled_but_never_lost() {
        let mut throttle = Throttle::default();
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        assert!(throttle.record(true, false, at(0)));
        assert!(!throttle.record(true, false, at(100)));
        assert!(throttle.record(false, true, at(200)));
        assert!(!throttle.record(false, true, at(5000)));
        assert!(throttle.record(true, false, at(6000)));
    }
}
