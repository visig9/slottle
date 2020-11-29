//! A throttle pool library designed for thread-based concurrency.
//!
//! # Concepts
//!
//! This crate contain two primary types: [`ThrottlePool`] and [`Throttle`].
//!
//! Each [`Throttle`] has it own concurrent and interval state for delaying.
//! On the other hand, [`ThrottlePool`] can automatic create [`Throttle`] when
//! corresponding `id` first time be used. User can treat `id` as some kind of
//! resource identity like hostname, IP address, etc.
//!
//! Here is a running chart of a [`ThrottlePool`] with `concurrent` == `2`.
//!
//! ```text
//! ThrottlePool
//!  |
//!  +-- Throttle (resource-1)
//!  |      |
//!  |      +-- Thread quota 1     ... run ...
//!  |      +-- Thread quota 2     ... run ...
//!  |
//!  +-- Throttle (resource-2)
//!  |      |
//!  |      +-- Thread quota 3     ... run ...
//!  |      +-- Thread quota 4     ... run ...
//!  ...
//!  +-- Throttle (resource-N)
//!         |
//!         +-- Thread quota 2N-1  ... run ...
//!         +-- Thread quota 2N    ... run ...
//! ```
//!
//!
//!
//! If `concurrent == 1`, thread quota usage may work like this:
//!
//! ```text
//! f: assigned jobs, s: sleep function
//!
//! thread 1:   |f()----|s()----|f()--|s()------|f()----------------|f()-----|..........|f()--
//!             |   interval    |   interval    |   interval    |...|   interval    |...|
//!                                             ^^^^^^^^^^^^^^^^^^^^^
//!             job run longer than interval --^                             ^^^^^^^^
//!             so skip sleep() step                                        /
//!                                                                        /
//!                 If new job not inject into the -----------------------
//!                 "should wait interval", sleep() will not be triggered
//!
//! time pass ----->
//! ```
//!
//!
//!
//! If `concurrent == 2`, threads will work like this:
//!
//! ```text
//! f: assigned jobs, s: sleep function
//!
//! thread 1:   |f()----|s()----|f()--|s()------|f()------------------------------|.|f()--
//!             |   interval    |   interval    |           2x interval         |...|
//!
//! thread 2:   |f()-|s()-------|f()-------|s()-|f()-|s()-------|f()|s()|f()---s|f()------
//!             |   interval    |   interval    |   interval    |  1/2  |  1/2  |
//!                                                             ^^^^^^^^^^^^^^^^^
//!                 max concurrent forced to 2  ---------------^
//!                 but expected value of maximux access speed is "concurrent per interval".
//!
//! time pass ----->
//! ```
//!
//! [`Throttle`] would not create threads, but only block current thread.
//! User should create threads by themself and sync throttle to all those
//! threads, to control access speed entirely.
//!
//! User can just using [`Throttle`] directly if not need the pool-related facility.
//!
//!
//!
//! # Examples
//!
//! ```rust
//! use rayon::prelude::*;
//! use slottle::ThrottlePool;
//! use std::time::{Duration, Instant};
//!
//! fn main() {
//!     // Make sure we have enough of threads can be blocked.
//!     // Here we use rayon as example but you can choice any thread implementation.
//!     rayon::ThreadPoolBuilder::new()
//!         .num_threads(8)
//!         .build_global()
//!         .unwrap();
//!
//!     // Create ThrottlePool.
//!     //
//!     // In here `id` is `bool` type for demonstration.
//!     // If you're writing a web spider, type of `id` might should be `url::Host`.
//!     let throttles: ThrottlePool<bool> = ThrottlePool::builder()
//!         .interval(Duration::from_millis(10)) // set interval to 10ms
//!         .concurrent(2) // set concurrent to 2
//!         .build()
//!         .unwrap();
//!
//!     // HINT: according previous config, expected access speed is
//!     // 2 per 10ms = 1 per 5ms (in each throttle)
//!
//!     let started_time = Instant::now();
//!
//!     let mut time_passed_ms_vec: Vec<f64> = vec![1, 2, 3, 4, 5, 6]
//!         .into_par_iter()
//!         .map(|x| {
//!             throttles.run(
//!                 x >= 5, // 5,6 in throttle `true` & 1,2,3,4 in throttle `false`
//!                 // here is the operation we want to throttling
//!                 || {
//!                     let time_passed_ms = started_time.elapsed().as_secs_f64() * 1000.0;
//!                     println!(
//!                         "[throttle: {:>5}] allowed job {} to start at: {:.2}ms",
//!                         x >= 5, x, time_passed_ms,
//!                     );
//!
//!                     // // you can also add some long-running task to see how throttle work
//!                     // std::thread::sleep(Duration::from_millis(20));
//!
//!                     time_passed_ms
//!                 },
//!             )
//!         })
//!         .collect();
//!
//!     // Verify all time_passed_ms as we expected...
//!
//!     time_passed_ms_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
//!
//!     let expected_time_passed_ms = [0.0, 0.0, 5.0, 5.0, 10.0, 15.0];
//!     time_passed_ms_vec
//!         .into_iter()
//!         .zip(expected_time_passed_ms.iter())
//!         .for_each(|(value, expected)| {
//!             assert_eq_approx("time_passed (ms)", value, *expected, 1.0)
//!         });
//! }
//!
//! /// assert value approximate equal to expected value.
//! fn assert_eq_approx(name: &str, value: f64, expected: f64, espilon: f64) {
//!     let expected_range = (expected - espilon)..(expected + espilon);
//!     assert!(
//!         expected_range.contains(&value),
//!         "{}: {} out of accpetable range: {:?}",
//!         name, value, expected_range,
//!     );
//! }
//! ```
//!
//! Output:
//!
//! ```text
//! [throttle: false] allowed job 1 to start at: 0.05ms
//! [throttle:  true] allowed job 5 to start at: 0.06ms
//! [throttle: false] allowed job 2 to start at: 5.10ms
//! [throttle:  true] allowed job 6 to start at: 5.12ms
//! [throttle: false] allowed job 3 to start at: 10.11ms
//! [throttle: false] allowed job 4 to start at: 15.11ms
//! ```
//!
//!
//!
//! # Features
//!
//! - `fuzzy_fns`: (optional) Offer helper function can fuzzing the `interval` in all operations.
//! - `retrying`: (optional, **experimental**) Add `run_retry(...)` APIs to support [retry] beside the standard throttle operation.
//!
//!
//!
//! # Naming
//!
//! Crate name `slottle` is the abbr of "slotted throttle". Which is the original name of `ThrottlePool`.

use std::{
    collections::HashMap,
    fmt::{self, Debug},
    hash::Hash,
    marker::PhantomData,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use std_semaphore::Semaphore;

type FuzzyFn = fn(interval: Duration) -> Duration;

/// A [`Throttle`] pool to restrict the resource access speed for multiple resources.
///
/// See [module](self) document for more detail.
pub struct ThrottlePool<K: Hash + Eq> {
    throttles: Mutex<HashMap<K, Arc<Throttle>>>,
    interval: Duration,
    concurrent: u32,
    fuzzy_fn: Option<FuzzyFn>,
}

impl<K: Hash + Eq> ThrottlePool<K> {
    /// Start to create a `ThrottlePool` by [`ThrottlePoolBuilder`].
    pub fn builder() -> ThrottlePoolBuilder<K> {
        ThrottlePoolBuilder::default()
    }

    /// Get a throttle from pool, if not exists, create it.
    fn get_throttle(&self, id: K) -> Arc<Throttle> {
        Arc::clone(
            self.throttles
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .entry(id)
                .or_insert_with(|| {
                    let mut builder = Throttle::builder();
                    builder.interval(self.interval).concurrent(self.concurrent);

                    if let Some(fuzzy_fn) = self.fuzzy_fn {
                        builder.fuzzy_fn(fuzzy_fn);
                    }

                    Arc::new(
                        builder
                            .build()
                            .expect("`concurrent` already varified when ThrottlePool created"),
                    )
                }),
        )
    }

    /// Run some function in particular throttle.
    ///
    /// This operation may block current thread by throttle current state and configuration.
    pub fn run<F, T>(&self, id: K, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let throttle = self.get_throttle(id);
        throttle.run(f)
    }

    /// > ***Experimental**: this API maybe deleted or re-designed in future version.*
    ///
    /// Run some function in particular throttle & retrying if function return `Err(T)`.
    ///
    /// Check [`Throttle::run_retry()`] for more detail.
    ///
    /// # Feature
    ///
    /// Only exists when `retrying` feature on.
    #[cfg(feature = "retrying")]
    #[cfg_attr(docsrs, doc(cfg(feature = "retrying")))]
    pub fn run_retry<F, T, E, R, OR>(&self, id: K, f: F, rseq: R) -> Result<T, E>
    where
        F: FnMut(u64) -> OR,
        OR: Into<retrying::OperationResult<T, E>>,
        R: IntoIterator<Item = Duration>,
    {
        let throttle = self.get_throttle(id);
        throttle.run_retry(f, rseq)
    }
}

impl<K: Hash + Eq> Debug for ThrottlePool<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Throttle")
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

/// Use it to build a [`ThrottlePool`].
pub struct ThrottlePoolBuilder<K: Hash + Eq> {
    interval: Duration,
    concurrent: u32,
    fuzzy_fn: Option<FuzzyFn>,
    phantom: PhantomData<K>,
}

impl<K: Hash + Eq> Default for ThrottlePoolBuilder<K> {
    fn default() -> Self {
        Self {
            interval: Duration::default(),
            concurrent: 1,
            fuzzy_fn: None,
            phantom: PhantomData,
        }
    }
}

impl<K: Hash + Eq> ThrottlePoolBuilder<K> {
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

    /// Create a [`ThrottlePool`] pool with previous configuration.
    ///
    /// Return `None` if `concurrent` == `0` or larger than `isize::MAX`.
    pub fn build(&mut self) -> Option<ThrottlePool<K>> {
        // check the configurations can initialize throttle properly.
        if let None = Throttle::builder()
            .interval(self.interval)
            .concurrent(self.concurrent)
            .build()
        {
            return None;
        }

        Some(ThrottlePool {
            throttles: Mutex::new(HashMap::new()),
            interval: self.interval,
            concurrent: self.concurrent,
            fuzzy_fn: self.fuzzy_fn.take(),
        })
    }
}

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
    /// use slottle::{Throttle, retrying};
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

/// Use it to build a [`Throttle`].
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

#[cfg(feature = "retrying")]
pub mod retrying {
    //! Re-export utils in [`retry`] crate for `retrying` feature related facility.
    //!
    //! # Note
    //!
    //! Only exists when `retrying` feature on.

    pub use retry::delay::{jitter, Exponential, Fibonacci, Fixed, NoDelay, Range};
    pub use retry::OperationResult;
}

#[cfg(feature = "fuzzy_fns")]
pub mod fuzzy_fns {
    //! Helper functions can assign into [`ThrottleBuilder::fuzzy_fn()`](crate::ThrottleBuilder::fuzzy_fn()).
    //!
    //! # Note
    //!
    //! Only exists when `fuzzy_fns` feature on.

    use rand::prelude::*;
    use std::time::Duration;

    /// Generate a new [`Duration`] between `0..(2 * interval)` by uniform distribution.
    pub fn uniform(interval: Duration) -> Duration {
        let jitter = random::<f64>() * 2.0;
        Duration::from_secs_f64(interval.as_secs_f64() * jitter)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::time::Duration;

        #[test]
        fn fuzzy_fn_uniform() {
            vec![Duration::from_secs(10); 100]
                .into_iter()
                .map(uniform)
                .for_each(|d| {
                    assert!((Duration::from_secs(0)..Duration::from_secs(20)).contains(dbg!(&d)))
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    #[test]
    fn throttle_with_concurrent_equal_0() {
        assert!(Throttle::builder().concurrent(0).build().is_none());
    }

    #[test]
    fn throttle_with_concurrent_equal_to_isize_max() {
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
    fn throttle_with_concurrent_large_than_isize_max() {
        assert!(Throttle::builder()
            // If isize::MAX > u32 (mean target_pointer_width = 64 or larger), just
            // cannot compile due to overflow.
            .concurrent(isize::MAX as u32 + 1)
            .build()
            .is_none());
    }

    #[test]
    fn throttle_with_fuzzy_fn() {
        Throttle::builder()
            .fuzzy_fn(|_| Duration::default())
            .build()
            .unwrap();
    }

    #[test]
    fn throttle_pool_run() {
        let throttles: ThrottlePool<u32> = ThrottlePool::builder()
            .interval(Duration::from_millis(1))
            .concurrent(2)
            .build()
            .unwrap();

        let results: Vec<i32> = vec![1, 2, 3]
            .into_par_iter()
            .map(|x| throttles.run(1, || x + 1))
            .collect();

        assert!(results == vec![2, 3, 4]);
    }

    #[test]
    #[cfg(feature = "retrying")]
    fn throttle_pool_run_retry() {
        let throttles: ThrottlePool<u32> = ThrottlePool::builder()
            .interval(Duration::from_millis(1))
            .concurrent(2)
            .build()
            .unwrap();

        let results: Vec<i32> = vec![1, 2, 3]
            .into_par_iter()
            .map(|x| {
                throttles.run_retry(
                    1,
                    |_| Ok(x + 1),
                    retrying::Fibonacci::from_millis(100).take(5),
                )
            })
            .collect::<Result<Vec<i32>, i32>>()
            .unwrap();

        assert!(results == vec![2, 3, 4]);
    }
}
