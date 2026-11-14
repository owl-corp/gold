//! Client-side rate limiting that follows the limits the server reports..
//!
//! GGG sends its rules in `x-rate-limit-*` headers on every response. The client keeps
//! a window per rule, waits when one is full, and adopts new rules as they appear.
use anyhow::{bail, Result};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug)]
struct Limiter {
    max: u32,
    window: Duration,
    /// when each request in the window frees its slot
    stack: Vec<Instant>,
}

impl Limiter {
    fn prune(&mut self, now: Instant) {
        self.stack.retain(|t| *t > now);
    }

    fn available(&self) -> u32 {
        self.max.saturating_sub(self.stack.len() as u32)
    }

    fn borrow(&mut self, now: Instant) {
        self.stack.push(now + self.window);
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
        Rules {
            limiters: Mutex::new(vec![Limiter {
                max: 1,
                window: Duration::from_secs(5),
                stack: vec![],
            }]),
        }
    }
}

impl Rules {
    /// Waits until every rule has room, then takes a slot in each.
    pub async fn wait(&self) {
        loop {
            let wait = {
                let mut limiters = self.limiters.lock().unwrap();
                let now = Instant::now();
                limiters.iter_mut().for_each(|l| l.prune(now));
                let blocked = limiters
                    .iter()
                    .filter(|l| l.available() == 0)
                    .filter_map(|l| l.stack.iter().min().map(|t| t.saturating_duration_since(now)))
                    .max();
                if blocked.is_none() {
                    limiters.iter_mut().for_each(|l| l.borrow(now));
                    return;
                }
                blocked.unwrap()
            };
            tokio::time::sleep(wait.max(Duration::from_millis(5))).await;
        }
    }

    /// `RateLimiter.estimateTime`: how long `count` requests would have to wait.
    pub fn estimate(&self, count: u32, ignore_state: bool) -> Duration {
        let now = Instant::now();
        let limiters = self.limiters.lock().unwrap();
        // times are relative to now, in seconds
        let mut sim: Vec<(u32, f64, Vec<f64>)> = limiters
            .iter()
            .map(|l| {
                let mut stack: Vec<f64> = if ignore_state {
                    vec![]
                } else {
                    l.stack
                        .iter()
                        .map(|t| t.saturating_duration_since(now).as_secs_f64())
                        .collect()
                };
                stack.sort_by(|a, b| a.partial_cmp(b).unwrap());
                (l.max, l.window.as_secs_f64(), stack)
            })
            .collect();

        let mut total = 0.0;
        for _ in 0..count {
            while sim.iter().any(|(max, _, stack)| stack.len() as u32 >= *max) {
                let wait = sim
                    .iter()
                    .filter(|(max, _, stack)| stack.len() as u32 >= *max)
                    .map(|(_, _, stack)| stack[0] - total)
                    .fold(0.0, f64::max);
                total += wait;
                for (_, _, stack) in sim.iter_mut() {
                    stack.retain(|t| *t > total);
                }
            }
            for (_, window, stack) in sim.iter_mut() {
                stack.push(total + *window);
            }
        }
        Duration::from_secs_f64(total.max(0.0))
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
                let max = p.next()?.parse().ok()?;
                let window: f64 = p.next()?.parse().ok()?;
                Some((max, Duration::from_secs_f64(window) + latency, state))
            })
            .collect();

        let mut limiters = self.limiters.lock().unwrap();
        let now = Instant::now();

        // forget rules the server no longer has
        limiters.retain(|l| server.iter().any(|(max, window, _)| l.is_equal(*max, *window)));

        // compare client and server state
        for l in limiters.iter_mut() {
            l.prune(now);
            let Some((_, _, state)) = server.iter().find(|(max, window, _)| l.is_equal(*max, *window)) else {
                continue;
            };
            let delta = *state as i64 - l.stack.len() as i64;
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
                stack: vec![],
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
mod tests {
    use super::*;

    fn rules(max: u32, secs: u64) -> Rules {
        Rules {
            limiters: Mutex::new(vec![Limiter {
                max,
                window: Duration::from_secs(secs),
                stack: vec![],
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
        let l = r.limiters.lock().unwrap();
        assert_eq!(l.len(), 2);
        assert!(l
            .iter()
            .any(|l| l.is_equal(5, Duration::from_secs(7)) && l.stack.len() == 2));
        assert!(l.iter().any(|l| l.is_equal(15, Duration::from_secs(17))));
    }

    #[test]
    fn refuses_long_queues() {
        let r = rules(1, 60);
        r.wait_sync_for_test();
        assert!(prevent_queue_creation(&[(&r, 1)]).is_err());
    }
}

#[cfg(test)]
impl Rules {
    fn wait_sync_for_test(&self) {
        let mut l = self.limiters.lock().unwrap();
        let now = Instant::now();
        l.iter_mut().for_each(|x| x.borrow(now));
    }
}
