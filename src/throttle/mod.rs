use std::{
    collections::VecDeque,
    fmt::{self, Debug},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use std_semaphore::Semaphore;

pub mod interval;

pub type IntervalFn = dyn Fn(Option<&ThrottleLog>) -> Duration + Send + Sync + 'static;

/// Limiting resource access speed by interval and concurrent.
pub struct Throttle {
    /// Which time point are allowed to perform the next `run()`.
    allowed_future: Mutex<Instant>,
    semaphore: Semaphore,
    log: Option<Mutex<ThrottleLog>>,
    interval_fn: Arc<IntervalFn>,
    concurrent: u32,
}

impl Throttle {
    /// Initialize a builder to create throttle.
    pub fn builder() -> ThrottleBuilder {
        ThrottleBuilder::new()
    }

    /// Run a function.
    ///
    /// Call `run(...)` may block current thread by throttle's state and configuration.
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
    /// let ans: Vec<u32> = vec![3, 2, 1]
    ///     .into_par_iter()
    ///     .map(|x| {
    ///         // parallel run here
    ///         throttle.run(|| x + 1)
    ///     })
    ///     .collect();
    ///
    /// assert_eq!(ans, vec![4, 3, 2]);
    /// ```
    pub fn run<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        // occupying single concurrency quota
        let _semaphore_guard = self.semaphore.access();

        self.waiting();
        let result = f();
        self.write_log(true);

        result
    }

    /// Run a function which are fallible.
    ///
    /// When `f` return an `Err`, throttle will treat this function run
    /// into "failed" state. Failure will counting by [`ThrottleLog`] and may change
    /// following delay intervals in current throttle scope by user defined [`Interval`]
    /// within [`ThrottleBuilder::interval()`].
    ///
    /// Call `run_fallible(...)` may block current thread by throttle's state and configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use rayon::prelude::*;
    /// use slottle::{Throttle,Interval};
    ///
    /// let throttle = Throttle::builder()
    ///     .interval(Interval::new(
    ///         |log| match log.unwrap().failure_count_cont() {
    ///             0 => Duration::from_millis(10), // if successful
    ///             _ => Duration::from_millis(50), // if failed
    ///         },
    ///         1, // log_size
    ///     ))
    ///     .build()
    ///     .unwrap();
    ///
    /// let started_time = Instant::now();
    ///
    /// vec![Result::<(), ()>::Err(()); 3]  // 3 Err here
    ///     .into_par_iter()
    ///     .for_each(|err| {
    ///         throttle.run_fallible(|| {
    ///             let time_passed_ms = started_time.elapsed().as_secs_f64() * 1000.0;
    ///             println!("time passed: {:.2}ms", time_passed_ms);
    ///             err
    ///         });
    ///     });
    /// ```
    ///
    /// The pervious code will roughly print:
    ///
    /// ```text
    /// time passed: 0.32ms
    /// time passed: 10.19ms
    /// time passed: 60.72ms
    /// ```
    ///
    ///
    ///
    /// ## Explanation: Data in [`ThrottleLog`] will delay one op to take effect
    ///
    /// If you read previous example and result carefully, You may notice first op
    /// failed but second op not immediate slowdown (50ms). The slowdown appeared on
    /// third. You may wonder what happen here?
    ///
    /// Say technically, all the following statements are true:
    ///
    /// 1. We known an op failed or not, only when it has finished.
    /// 2. Current implementation of `Throttle` try to do "waiting" *just before* an op start.
    ///     - If put waiting *after* an op finished, final op may blocking the thread unnecessarily.
    /// 3. The "next allowed timepoint" must assigned with "waiting" as an atomic unit.
    ///     - If not, in multi-thread situation, more than one op may retrieve the same "allowed
    ///     timepoint", then run in the same time.
    ///
    /// So, combine those 3 points. When op 1 finished and [`ThrottleLog`] updating, "next allowed timepoint"
    /// already be calculated for other pending ops (those ops may started before current op finished if
    /// `concurrent >= 2`). But it looking little weird when `concurrent == 1`.
    ///
    /// Here is the chart:
    ///
    /// ```text
    /// f: assigned jobs, s: sleep function
    ///
    /// thread 1:   |f1()---|s()----|f2()--|s()---------------------------------|f3()---|.......
    ///             |   int.succ    |           interval (failed)               |...............
    ///             ^       ^       ^-- at this point throttle determined which time f3 allowed to run
    ///              \       \
    ///               \        -- f1 finished, now throttle known f1 failed, write into the log
    ///                \
    ///                  -- at this point throttle determined "which time f2 allowed to run"
    ///
    /// time pass ----->
    /// ```
    ///
    /// Thus, data in [`ThrottleLog`] will delay one op to take effect (no matter how many concurrent).
    pub fn run_fallible<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        // occupying single concurrency quota
        let _semaphore_guard = self.semaphore.access();

        self.waiting();
        let result = f();
        self.write_log(result.is_ok());

        result
    }

    /// Run a function and retry when it failed.
    ///
    /// If `f` return an `Result::Err`, throttle will auto re-run the function. Retry will
    /// happen again and again until it reach `max_retry` limitation or succeed.
    /// For example, assume `max_retry == 4` that `f` may run `5` times as maximum.
    ///
    /// This method may effect intervals calculation due to any kind of `Err` happened.
    /// Check [`run_fallible()`](Self::run_fallible) to see how it work.
    ///
    /// Call `retry(...)` may block current thread by throttle's state and configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use rayon::prelude::*;
    /// use slottle::Throttle;
    ///
    /// let throttle = Throttle::builder().build().unwrap();
    ///
    /// let which_round_finished: Vec<Result<_, _>> = vec![2, 1, 0]
    ///     .into_par_iter()
    ///     .map(|x| {
    ///         throttle.retry(
    ///             // round always in `1..=(max_retry + 1)` (`1..=2` in this case)
    ///             |round| match x + round >= 3 {
    ///                 false => Err(round),
    ///                 true => Ok(round),
    ///             },
    ///             1,  // max_retry == 1
    ///         )
    ///     })
    ///     .collect();
    ///
    /// assert_eq!(which_round_finished, vec![Ok(1), Ok(2), Err(2)]);
    /// ```
    ///
    /// Function `f` can also return [`RetryableResult::FatalErr`] to ask throttle don't do any
    /// further retry:
    ///
    /// ```
    /// use std::time::Duration;
    /// use rayon::prelude::*;
    /// use slottle::{Throttle, RetryableResult};
    ///
    /// let throttle = Throttle::builder().build().unwrap();
    ///
    /// let which_round_finished: Vec<Result<_, _>> = vec![2, 1, 0]
    ///     .into_par_iter()
    ///     .map(|x| {
    ///         throttle.retry(
    ///             // round always in `1..=(max_retry + 1)` (`1..=2` in this case)
    ///             |round| match x + round >= 3 {
    ///                 // FatalErr would not retry
    ///                 false => RetryableResult::FatalErr(round),
    ///                 true => RetryableResult::Ok(round),
    ///             },
    ///             1,  // max_retry == 1
    ///         )
    ///     })
    ///     .collect();
    ///
    /// assert_eq!(which_round_finished, vec![Ok(1), Err(1), Err(1)]);
    /// ```
    ///
    pub fn retry<F, T, E, R>(&self, mut f: F, max_retry: usize) -> Result<T, E>
    where
        F: FnMut(usize) -> R,
        R: Into<RetryableResult<T, E>>,
    {
        let max_try = max_retry + 1;
        let mut round = 1;

        loop {
            // occupying single concurrency quota
            let _semaphore_guard = self.semaphore.access();

            self.waiting();

            let result: RetryableResult<T, E> = f(round).into();
            match result {
                RetryableResult::Ok(v) => {
                    self.write_log(true);
                    return Ok(v);
                }
                RetryableResult::RetryableErr(e) => {
                    self.write_log(false);

                    if round == max_try {
                        return Err(e);
                    } else {
                        round += 1;
                    }
                }
                RetryableResult::FatalErr(e) => {
                    self.write_log(false);
                    return Err(e);
                }
            };
        }
    }

    fn waiting(&self) {
        // renew allow_future & calculate how long to wait further
        let still_should_wait: Option<Duration> = {
            let mut allowed_future_guard = self
                .allowed_future
                .lock()
                .expect("mutex impossible to be poison");

            // generate next interval
            let next_interval: Duration = (self.interval_fn)(
                self.log
                    .as_ref()
                    .map(|log| log.lock().expect("mutex impossible to be poison"))
                    .as_deref(),
            ) / self.concurrent;

            // get old allow_future
            let allowed_future = *allowed_future_guard;

            // Instant::now() should be called after the lock acquired or else may inaccurate.
            let now = Instant::now();

            // counting next_allowed_future from when?
            let next_allowed_future_baseline = *[now, allowed_future]
                .iter()
                .max()
                .expect("this is [Instant; 2] array so max value always exists");

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

    fn write_log(&self, successful: bool) {
        if let Some(log) = self.log.as_ref() {
            log.lock()
                .expect("mutex impossible to be poison")
                .push(LogRecord {
                    time: Instant::now(),
                    successful,
                });
        }
    }
}

impl Debug for Throttle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Throttle")
            .field("allowed_future", &self.allowed_future)
            .field("concurrent", &self.concurrent)
            .finish()
    }
}

/// Use to build a [`Throttle`].
///
/// Created by [`Throttle::builder()`] API.
pub struct ThrottleBuilder {
    interval_fn: Arc<IntervalFn>,
    concurrent: u32,
    log_size: usize,
}

impl ThrottleBuilder {
    fn new() -> Self {
        Self {
            interval_fn: Arc::new(|_| Duration::default()),
            concurrent: 1,
            log_size: 0,
        }
    }

    /// Set interval of throttle.
    ///
    /// # Example
    ///
    /// ```
    /// use slottle::{Throttle, Interval};
    /// use std::time::Duration;
    /// use rand;
    ///
    /// // fixed interval: 10ms
    /// Throttle::builder().interval(Duration::from_millis(10));
    ///
    /// // random interval between: 10ms ~ 0ms
    /// Throttle::builder()
    ///     .interval(|| Duration::from_millis(10).mul_f64(rand::random()));
    ///
    /// // increasing delay if failed continuously
    /// Throttle::builder()
    ///     .interval(Interval::new(
    ///         |log| match log.unwrap().failure_count_cont() {
    ///             0 => Duration::from_millis(10),
    ///             1 => Duration::from_millis(30),
    ///             2 => Duration::from_millis(50),
    ///             3 => Duration::from_millis(70),
    ///             _ => unreachable!(),
    ///         },
    ///         3,  // maximum log size
    ///     ));
    ///
    /// // use pre-defined algorithm
    /// Throttle::builder()
    ///     .interval(slottle::fibonacci(
    ///         Duration::from_millis(10),
    ///         Duration::from_secs(2),
    ///     ));
    /// ```
    pub fn interval<A>(&mut self, a: A) -> &mut Self
    where
        A: Into<Interval>,
    {
        let a = a.into();

        self.interval_fn = a.interval_fn;
        self.log_size = a.log_size;
        self
    }

    /// Set concurrent, default value is `1`.
    pub fn concurrent(&mut self, concurrent: u32) -> &mut Self {
        self.concurrent = concurrent;
        self
    }

    /// Create a new [`Throttle`] with current configuration.
    ///
    /// Return `None` if `concurrent` == `0` or larger than `isize::MAX`.
    pub fn build(&mut self) -> Option<Throttle> {
        use std::convert::TryInto;

        if self.concurrent == 0 {
            return None;
        }

        Some(Throttle {
            allowed_future: Mutex::new(Instant::now()),
            log: match self.log_size {
                0 => None,
                _ => Some(Mutex::new(ThrottleLog::new(self.log_size))),
            },
            semaphore: Semaphore::new(self.concurrent.try_into().ok()?),
            interval_fn: Arc::clone(&self.interval_fn),
            concurrent: self.concurrent,
        })
    }
}

impl Debug for ThrottleBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThrottleBuilder")
            .field("concurrent", &self.concurrent)
            .field("log_size", &self.log_size)
            .finish()
    }
}

/// The result type for [`Throttle::retry()`] API.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum RetryableResult<T, E> {
    /// Represent operation successful.
    Ok(T),
    /// Represent operation failed & allow to retry.
    RetryableErr(E),
    /// Represent operation failed & should not retry.
    FatalErr(E),
}

impl<T, E> From<Result<T, E>> for RetryableResult<T, E> {
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::RetryableErr(e),
        }
    }
}

/// Collect operation log of a [`Throttle`].
///
/// User can access this log by [`ThrottleBuilder::interval()`] API by
/// [`Interval`].
///
/// `ThrottleLog` will drop oldest log records automatically when it reach
/// it size limit.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ThrottleLog {
    size: usize,
    inner: VecDeque<LogRecord>,
}

impl ThrottleLog {
    fn new(size: usize) -> Self {
        Self {
            size,
            inner: VecDeque::with_capacity(size),
        }
    }

    fn push(&mut self, log_record: LogRecord) {
        // if size == 0, noop
        if self.size == 0 {
            return;
        }

        // if size != 0 and already full, remove oldest before insert record
        if self.size == self.inner.len() {
            self.inner.pop_back();
        }

        self.inner.push_front(log_record);
    }

    /// Get maximum log size.
    ///
    /// This value would never change.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get how many failures exists in log.
    ///
    /// # Example
    ///
    /// (Left is new, right is old, F = Failure, S = Successful)
    ///
    /// - `FFFFF`: 5
    /// - `FFSFF`: 4
    /// - `SFFFF`: 4
    /// - `FSSSS`: 1
    pub fn failure_count(&self) -> usize {
        self.inner
            .iter()
            .filter(|record| !record.successful)
            .count()
    }

    /// Get how many failures from newest log entry continuously.
    ///
    /// # Example
    ///
    /// (Left is new, right is old, F = Failure, S = Successful)
    ///
    /// - `FFFFF`: 5
    /// - `FFSFF`: 2
    /// - `SFFFF`: 0
    /// - `FSSSS`: 1
    pub fn failure_count_cont(&self) -> usize {
        self.inner
            .iter()
            .take_while(|record| !record.successful)
            .count()
    }

    /// Get failure rate in whole log.
    ///
    /// # Example
    ///
    /// (Left is new, right is old, F = Failure, S = Successful)
    ///
    /// - `FFFFF`: 1.0
    /// - `FFSFF`: 0.8
    /// - `SFFFF`: 0.8
    /// - `FSSSS`: 0.2
    ///
    /// This function use `size` as denominator. Return `None` if `size == 0`.
    pub fn failure_rate(&self) -> Option<f64> {
        if self.size == 0 {
            None
        } else {
            let failed_count = self.failure_count();

            Some(failed_count as f64 / self.size as f64)
        }
    }

    /// Get duration between first and last log record.
    ///
    /// Return `None` if don't have at least 2 log records.
    pub fn duration(&self) -> Option<Duration> {
        if self.inner.len() <= 1 {
            None
        } else {
            Some(self.inner.front().unwrap().time - self.inner.back().unwrap().time)
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogRecord {
    time: Instant,
    successful: bool,
}

/// The interval configuration.
#[derive(Clone)]
pub struct Interval {
    interval_fn: Arc<IntervalFn>,
    log_size: usize,
}

impl Interval {
    /// Create a interval calculating algorithm
    ///
    /// Define an `interval_fn` to generate interval dynamically.
    ///
    /// `log_size` argument determine the maximum size of [`ThrottleLog`] which
    /// `interval_fn` can access. If `log_size == 0`, `interval_fn` will receive
    /// `None`.
    pub fn new<F>(interval_fn: F, log_size: usize) -> Self
    where
        F: Fn(Option<&ThrottleLog>) -> Duration + Send + Sync + 'static,
    {
        Self {
            interval_fn: Arc::new(interval_fn),
            log_size,
        }
    }

    /// Apply post-process to generated interval.
    ///
    /// This method are useful when user want to do some tweaks with
    /// pre-built interval algorithm.
    ///
    /// # example
    ///
    /// ```
    /// use std::time::Duration;
    /// use slottle::Interval;
    ///
    /// // following algorithm produce random duration from 0 ~ 10ms
    /// let algo = Interval::new(|_| Duration::from_millis(10), 0)
    ///     .modify(|dur| dur * rand::random());
    /// ```
    pub fn modify<F>(self, f: F) -> Interval
    where
        F: Fn(Duration) -> Duration + Send + Sync + 'static,
    {
        let orig_fn = self.interval_fn;

        Self {
            interval_fn: Arc::new(move |log| f(orig_fn(log))),
            log_size: self.log_size,
        }
    }
}

impl<F> From<F> for Interval
where
    F: Fn() -> Duration + Send + Sync + 'static,
{
    fn from(f: F) -> Self {
        Self {
            interval_fn: Arc::new(move |_| f()),
            log_size: 0,
        }
    }
}

impl From<Duration> for Interval {
    fn from(duration: Duration) -> Self {
        Self {
            interval_fn: Arc::new(move |_| duration),
            log_size: 0,
        }
    }
}

impl Debug for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Interval")
            .field("log_size", &self.log_size)
            .finish()
    }
}

impl Default for Interval {
    fn default() -> Self {
        Self {
            interval_fn: Arc::new(|_| Duration::default()),
            log_size: 0,
        }
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
    fn retryable_result_convert() {
        let orig: Result<bool, u32> = Err(42);
        let to: RetryableResult<bool, u32> = orig.into();

        assert_eq!(to, RetryableResult::RetryableErr(42))
    }

    #[test]
    fn throttle_log_op() {
        let mut log = ThrottleLog::new(4);

        assert_eq!(log.failure_count_cont(), 0);
        assert_eq!(log.failure_count(), 0);
        assert_eq!(log.failure_rate().unwrap(), 0.0);

        log.push(LogRecord {
            time: Instant::now(),
            successful: false,
        });
        assert_eq!(log.failure_count_cont(), 1);
        assert_eq!(log.failure_count(), 1);
        assert_eq!(log.failure_rate().unwrap(), 0.25);

        log.push(LogRecord {
            time: Instant::now(),
            successful: false,
        });
        assert_eq!(log.failure_count_cont(), 2);
        assert_eq!(log.failure_count(), 2);
        assert_eq!(log.failure_rate().unwrap(), 0.5);

        log.push(LogRecord {
            time: Instant::now(),
            successful: true,
        });
        log.push(LogRecord {
            time: Instant::now(),
            successful: true,
        });
        assert_eq!(log.failure_count_cont(), 0);
        assert_eq!(log.failure_count(), 2);
        assert_eq!(log.failure_rate().unwrap(), 0.5);

        log.push(LogRecord {
            time: Instant::now(),
            successful: true,
        });
        assert_eq!(log.failure_count_cont(), 0);
        assert_eq!(log.failure_count(), 1);
        assert_eq!(log.failure_rate().unwrap(), 0.25);

        log.push(LogRecord {
            time: Instant::now(),
            successful: false,
        });
        assert_eq!(log.failure_count_cont(), 1);
        assert_eq!(log.failure_count(), 1);
        assert_eq!(log.failure_rate().unwrap(), 0.25);
    }

    #[test]
    fn throttle_log_new_0() {
        let mut log = ThrottleLog::new(0);

        log.push(LogRecord {
            time: Instant::now(),
            successful: false,
        });
        assert_eq!(log.failure_count_cont(), 0);
        assert_eq!(log.failure_count(), 0);
        assert!(log.failure_rate().is_none());
    }

    #[test]
    fn interval_modify() {
        let algo = Interval::new(|_| Duration::from_millis(10), 0).modify(|dur| dur * 2);

        assert_eq!((algo.interval_fn)(None), Duration::from_millis(20));
    }
}
