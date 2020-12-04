use std::{
    fmt::{self, Debug},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use std_semaphore::Semaphore;

use crate::FuzzyFn;

#[cfg(feature = "retrying")]
use crate::retrying;

/// Limiting resource access speed by interval and concurrent.
pub struct Throttle {
    /// Which time point are allowed to perform the next `run()`.
    allowed_future: Mutex<Instant>,
    semaphore: Semaphore,

    interval: Duration,
    concurrent: u32,

    fuzzy_fn: Option<FuzzyFn>,
}

impl Throttle {
    pub fn builder() -> ThrottleBuilder {
        ThrottleBuilder::new()
    }

    /// Run some function.
    ///
    /// Call `run(...)` may block current thread by throttle current state and configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use rayon::prelude::*;
    /// use slottle::Throttle;
    ///
    /// let throttle = Throttle::builder()
    ///     .interval(Duration::from_millis(5))
    ///     .build()
    ///     .unwrap();
    ///
    /// let which_round_success: Vec<u32> = vec![3, 2, 1]
    ///     .into_par_iter()
    ///     .map(|x| {
    ///         throttle.run(|| x + 1)  // here
    ///     })
    ///     .collect();
    ///
    /// assert_eq!(which_round_success, vec![4, 3, 2]);
    /// ```
    pub fn run<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        // occupying single concurrency quota
        let _semaphore_guard = self.semaphore.access();

        self.waiting();

        f()
    }

    /// > ***Experimental**: this API maybe deleted or re-designed in future version.*
    ///
    /// Run some function & retrying if function return `Err(T)`.
    ///
    /// Call `run_retry(...)` may block current thread by throttle current state and configuration.
    /// When retrying, the occupied concurrent quota will not release in the half way:
    /// It will continue occupying until the job fully failed or success finally.
    ///
    /// The `rseq` is a "retry delay iterator" which control how many times it can
    /// retry and each retry's duration of delay.
    ///
    /// # Run chart
    ///
    /// ```text
    /// f: assigned jobs, s: sleep function
    /// rn: n-retry, sn: delay of n-retry
    ///
    /// thread 1:   |f()--|s()------|f()--s1--r1--s2--r2--s3----r3--|f()-----|......|..
    ///             |   interval    |   interval    |...............|   interval    |
    ///                                   ^^^^    ^^^^    ^^^^^^
    ///                 Each retry-delay-duration control by the "retry delay iterator" (rseq).
    ///                 Retrying algorithm wouldn't add `interval` between two of retries.
    ///                 Throttle wouldn't interrupt the retrying sequence by other pending jobs.
    ///
    /// time pass ----->
    /// ```
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use rayon::prelude::*;
    /// use slottle::{Throttle, retrying};
    ///
    /// let throttle = Throttle::builder().build().unwrap();
    ///
    /// let which_round_success: Vec<Result<u64, _>> = vec![3, 2, 1]
    ///     .into_par_iter()
    ///     .map(|x| {
    ///         throttle.run_retry(
    ///             // this op may failed
    ///             |round| match x + round >= 4 {
    ///                 false => Err(()),
    ///                 true => Ok(round),
    ///             },
    ///             // rseq: each retry delay 10 ms, can retry only 1 time (max round == 2).
    ///             retrying::Fixed::from_millis(10).take(1),
    ///         )
    ///     })
    ///     .collect();
    ///
    /// assert_eq!(which_round_success, vec![Ok(1), Ok(2), Err(())]);
    /// ```
    ///
    /// # Note
    ///
    /// If user want to apply `interval` to each retry, and allow other pending jobs
    /// injected between two of retries, please don't use this API. Just re-run `throttle.run(op)`
    /// multiple times by youself.
    ///
    /// # Feature
    ///
    /// Only exists when `retrying` feature on.
    #[cfg(feature = "retrying")]
    #[cfg_attr(docsrs, doc(cfg(feature = "retrying")))]
    pub fn run_retry<F, T, E, R, OR>(&self, f: F, rseq: R) -> Result<T, E>
    where
        F: FnMut(u64) -> OR,
        OR: Into<retrying::OperationResult<T, E>>,
        R: IntoIterator<Item = Duration>,
    {
        // occupying single concurrency quota
        let _semaphore_guard = self.semaphore.access();

        self.waiting();

        retry::retry_with_index(rseq, f).map_err(|retry_err| match retry_err {
            retry::Error::Operation { error, .. } => error,
            retry::Error::Internal(_) => unreachable!(),
        })
    }

    fn waiting(&self) {
        // renew allow_future & calculate how long to wait further
        let still_should_wait: Option<Duration> = {
            let mut allowed_future_guard = self
                .allowed_future
                .lock()
                .expect("mutex impossible to be poison");

            // get old allow_future
            let allowed_future = *allowed_future_guard;

            // Instant::now() should be called after the lock acquired or else may inaccurate.
            let now = Instant::now();

            // counting next_allowed_future from when?
            let next_allowed_future_baseline = *[now, allowed_future]
                .iter()
                .max()
                .expect("this is [Instant; 2] array so max value always exists");

            // process the interval by noice generator
            let next_interval: Duration = match self.fuzzy_fn {
                Some(fuzzy_fn) => fuzzy_fn(self.interval),
                None => self.interval,
            } / self.concurrent;

            let next_allowed_future = next_allowed_future_baseline + next_interval;
            *allowed_future_guard = next_allowed_future;

            drop(allowed_future_guard);

            allowed_future.checked_duration_since(now)
        };

        // sleep still_should_wait in this period
        if let Some(still_should_wait) = still_should_wait {
            thread::sleep(still_should_wait);
        }
    }
}

impl Debug for Throttle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Throttle")
            .field("allowed_future", &self.allowed_future)
            .field("interval", &self.interval)
            .field("concurrent", &self.concurrent)
            .field(
                "fuzzy_fn",
                &match self.fuzzy_fn.is_some() {
                    true => "Given",
                    false => "Not Exists",
                },
            )
            .finish()
    }
}

/// Use to build a [`Throttle`].
///
/// Create by [`Throttle::builder()`] API.
pub struct ThrottleBuilder {
    interval: Duration,
    concurrent: u32,
    fuzzy_fn: Option<FuzzyFn>,
}

impl ThrottleBuilder {
    fn new() -> Self {
        Self {
            interval: Duration::default(),
            concurrent: 1,
            fuzzy_fn: None,
        }
    }

    /// Set interval, default value is `Duration::default()`.
    pub fn interval(&mut self, interval: Duration) -> &mut Self {
        self.interval = interval;
        self
    }

    /// Set concurrent, default value is `1`.
    pub fn concurrent(&mut self, concurrent: u32) -> &mut Self {
        self.concurrent = concurrent;
        self
    }

    /// Set fuzzy_fn to tweak `interval` for each run, by default no fuzzy_fn
    /// (interval be used as is).
    ///
    /// # Feature
    ///
    /// If you enable `fuzzy_fns` then [`crate::fuzzy_fns`] will contain some fuzzy_fn implementations.
    pub fn fuzzy_fn(&mut self, fuzzy_fn: FuzzyFn) -> &mut Self {
        self.fuzzy_fn = Some(fuzzy_fn);
        self
    }

    /// Create a new throttle. Return `None` if `concurrent` == `0` or larger than `isize::MAX`.
    pub fn build(&mut self) -> Option<Throttle> {
        use std::convert::TryInto;

        if self.concurrent == 0 {
            return None;
        }

        Some(Throttle {
            allowed_future: Mutex::new(Instant::now()),
            semaphore: Semaphore::new(self.concurrent.try_into().ok()?),
            interval: self.interval,
            concurrent: self.concurrent,
            fuzzy_fn: self.fuzzy_fn.take(),
        })
    }
}

impl Debug for ThrottleBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThrottleBuilder")
            .field("concurrent", &self.concurrent)
            .field("interval", &self.interval)
            .field(
                "fuzzy_fn",
                &match self.fuzzy_fn.is_some() {
                    true => "Given",
                    false => "Not Exists",
                },
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_concurrent_equal_0() {
        assert!(Throttle::builder().concurrent(0).build().is_none());
    }

    #[test]
    fn with_concurrent_equal_to_isize_max() {
        // this case may run out of memory in previous implementation.
        assert!(Throttle::builder()
            .concurrent(isize::MAX as u32)
            .build()
            .is_some());
    }

    #[test]
    #[cfg(any(
        target_pointer_width = "8",
        target_pointer_width = "16",
        target_pointer_width = "32",
    ))]
    fn with_concurrent_large_than_isize_max() {
        assert!(Throttle::builder()
            // If isize::MAX > u32 (mean target_pointer_width = 64 or larger), just
            // cannot compile due to overflow.
            .concurrent(isize::MAX as u32 + 1)
            .build()
            .is_none());
    }

    #[test]
    fn with_fuzzy_fn() {
        Throttle::builder()
            .fuzzy_fn(|_| Duration::default())
            .build()
            .unwrap();
    }
}
