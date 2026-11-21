//! Client-side rate limiting that follows the limits the server reports..
//!
//! GGG sends its rules in `x-rate-limit-*` headers on every response. The client keeps
//! a window per rule, waits when one is full, and adopts new rules as they appear.
use anyhow::{bail, Result};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug)]
struct Limiter {
    max: u32,
    window: Duration,
    /// when each request in the window frees its slot
    stack: VecDeque<Instant>,
}

impl Limiter {
    fn prune(&mut self, now: Instant) {
        while self.stack.front().is_some_and(|t| *t <= now) {
            self.stack.pop_front();
        }
    }

    fn available(&self) -> u32 {
        self.max.saturating_sub(self.stack.len() as u32)
    }

    fn borrow(&mut self, now: Instant) {
        self.stack.push_back(now + self.window);
    }

    fn is_equal(&self, max: u32, window: Duration) -> bool {
        self.max == max && self.window == window
    }
}

/// One server rule set (search, exchange or fetch): every rule applies to every request.
pub struct Rules {
    limiters: Mutex<Vec<Limiter>>,
}

impl Default for Rules {
    fn default() -> Self {
        // a cautious start until the first response tells us the real limits
        Self {
            limiters: Mutex::new(vec![Limiter {
                max: 1,
                window: Duration::from_secs(5),
                stack: VecDeque::new(),
            }]),
        }
    }
}

impl Rules {
    /// Waits until every rule has room, then takes a slot in each.
    pub async fn wait(&self) {
        loop {
            let wait = {
                let mut limiters = self.limiters.lock();
                let now = Instant::now();
                limiters.iter_mut().for_each(|l| l.prune(now));
                let blocked = limiters
                    .iter()
                    .filter(|l| l.available() == 0)
                    .filter_map(|l| l.stack.front().map(|t| t.saturating_duration_since(now)))
                    .max();
                let Some(wait) = blocked else {
                    limiters.iter_mut().for_each(|l| l.borrow(now));
                    drop(limiters);
                    return;
                };
                drop(limiters);
                wait
            };
            tokio::time::sleep(wait.max(Duration::from_millis(5))).await;
        }
    }

    /// `RateLimiter.estimateTime`: how long `count` requests would have to wait.
    pub fn estimate(&self, count: u32, ignore_state: bool) -> Duration {
        let now = Instant::now();
        let limiters = self.limiters.lock();
        // times are offsets from now; each stack is sorted because the window is fixed
        let mut sim: Vec<(u32, Duration, VecDeque<Duration>)> = limiters
            .iter()
            .map(|l| {
                let stack = if ignore_state {
                    VecDeque::new()
                } else {
                    l.stack.iter().map(|t| t.saturating_duration_since(now)).collect()
                };
                (l.max, l.window, stack)
            })
            .collect();
        drop(limiters);

        let full = |max: u32, stack: &VecDeque<Duration>| stack.len() >= max as usize;
        let mut total = Duration::ZERO;
        for _ in 0..count {
            while sim.iter().any(|(max, _, stack)| full(*max, stack)) {
                let wait = sim
                    .iter()
                    .filter(|(max, _, stack)| full(*max, stack))
                    .filter_map(|(_, _, stack)| stack.front().map(|t| t.saturating_sub(total)))
                    .max()
                    .unwrap_or(Duration::ZERO);
                total += wait;
                for (_, _, stack) in &mut sim {
                    while stack.front().is_some_and(|t| *t <= total) {
                        stack.pop_front();
                    }
                }
            }
            for (_, window, stack) in &mut sim {
                stack.push_back(total + *window);
            }
        }
        total
    }

    /// `adjustRateLimits`: adopt the rules the server reported.
    /// `limits` is e.g. "5:5:10,15:15:60" (max:window:penalty), `states` the matching
    /// "1:5:0,3:15:0" (used:window:penalty). `latency` pads each window against clock drift.
    pub fn adjust(&self, limits: &str, states: &str, latency: Duration) {
        let server: Vec<(u32, Duration, u32)> = limits
            .split(',')
            .zip(
                states
                    .split(',')
                    .map(|s| s.split(':').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0)),
            )
            .filter_map(|(rule, state)| {
                let mut p = rule.split(':');
                let max: u32 = p.next()?.parse().ok().filter(|m| *m > 0)?;
                let window: f64 = p.next()?.parse().ok()?;
                Some((max, Duration::try_from_secs_f64(window).ok()? + latency, state))
            })
            .collect();

        let mut limiters = self.limiters.lock();
        let now = Instant::now();

        // forget rules the server no longer has
        limiters.retain(|l| server.iter().any(|(max, window, _)| l.is_equal(*max, *window)));

        // compare client and server state
        for l in limiters.iter_mut() {
            l.prune(now);
            let Some((_, _, state)) = server.iter().find(|(max, window, _)| l.is_equal(*max, *window)) else {
                continue;
            };
            let delta = i64::from(*state) - i64::try_from(l.stack.len()).unwrap_or(i64::MAX);
            if delta > 0 {
                // the server has seen more requests than we have: burst to match
                for _ in 0..(delta as u32).min(l.available()) {
                    l.borrow(now);
                }
            }
        }

        // add new rules
        for (max, window, state) in server {
            if limiters.iter().any(|l| l.is_equal(max, window)) {
                continue;
            }
            let mut l = Limiter {
                max,
                window,
                stack: VecDeque::new(),
            };
            for _ in 0..state {
                l.borrow(now);
            }
            limiters.push(l);
        }
    }
}

/// `preventQueueCreation`: refuse a request that would have to queue for long, instead
/// of leaving the user watching a spinner.
pub fn prevent_queue_creation(targets: &[(&Rules, u32)]) -> Result<()> {
    let estimated = targets
        .iter()
        .map(|(rules, count)| {
            let est = rules.estimate(*count, false);
            let clean = rules.estimate(*count, true);
            // ignore if impossible to run without queue
            if est == clean {
                Duration::ZERO
            } else {
                est
            }
        })
        .max()
        .unwrap_or(Duration::ZERO);

    if estimated >= Duration::from_millis(1500) {
        bail!("Retry after {} seconds", estimated.as_secs_f64().round());
    }
    Ok(())
}

#[cfg(test)]
impl Rules {
    fn wait_sync_for_test(&self) {
        let mut l = self.limiters.lock();
        let now = Instant::now();
        l.iter_mut().for_each(|x| x.borrow(now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(max: u32, secs: u64) -> Rules {
        Rules {
            limiters: Mutex::new(vec![Limiter {
                max,
                window: Duration::from_secs(secs),
                stack: VecDeque::new(),
            }]),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_a_free_slot() {
        let r = rules(2, 10);
        let start = tokio::time::Instant::now();
        r.wait().await;
        r.wait().await;
        assert!(start.elapsed() < Duration::from_secs(1));
        r.wait().await; // the third has to wait for the first slot
        assert!(start.elapsed() >= Duration::from_secs(10));
    }

    #[test]
    fn estimates_queueing() {
        let r = rules(1, 5);
        assert_eq!(r.estimate(1, false), Duration::ZERO);
        assert_eq!(r.estimate(2, false), Duration::from_secs(5));
        assert_eq!(r.estimate(3, true), Duration::from_secs(10));
    }

    #[test]
    fn adopts_server_rules() {
        let r = Rules::default();
        r.adjust("5:5:10,15:15:60", "2:5:0,2:15:0", Duration::from_secs(2));
        let l = r.limiters.lock();
        assert_eq!(l.len(), 2);
        assert!(l
            .iter()
            .any(|l| l.is_equal(5, Duration::from_secs(7)) && l.stack.len() == 2));
        assert!(l.iter().any(|l| l.is_equal(15, Duration::from_secs(17))));
        drop(l);
    }

    #[test]
    fn refuses_long_queues() {
        let r = rules(1, 60);
        r.wait_sync_for_test();
        assert!(prevent_queue_creation(&[(&r, 1)]).is_err());
    }
}
