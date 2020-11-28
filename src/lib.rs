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
//! use std::time::Duration;
//! use slottle::ThrottlePool;
//! use rayon::prelude::*;
//!
//! // Create ThrottlePool to store some state.
//! //
//! // In here `id` is `bool` type for demonstration. If you're writing
//! // a web spider, type of `id` might be `url::Host`.
//! let throttles: ThrottlePool<bool> =
//!     ThrottlePool::builder()
//!         .interval(Duration::from_millis(1)) // set interval to 1 ms
//!         .concurrent(2)                      // 2 concurrent for each throttle
//!         .build()
//!         .unwrap();
//!
//! // make sure you have enough of threads. For example:
//! rayon::ThreadPoolBuilder::new().num_threads(8).build_global().unwrap();
//!
//! let results: Vec<i32> = vec![1, 2, 3, 4, 5]
//!     .into_par_iter()        // run parallel
//!     .map(|x| throttles.run(
//!         x == 5,             // 5 in throttle `true`, 1,2,3,4 in throttle `false`
//!         || {x + 1},         // here is the operation should be throttled
//!     ))
//!     .collect();
//!
//! assert_eq!(results, vec![2, 3, 4, 5, 6,]);
//! ```
//!
//!
//!
//! # Features
//!
//! - `fuzzy_fns`: (default) Offer a helper function can fuzzing the `interval` in all operations.
//! - `retrying`: (optional, **experimental**) Add `run_retry(...)` APIs to support [retry] beside the standard throttle operation.
//!
//!
//!
//! # Other
//!
//! Crate name `slottle` is an abbr of "slotted throttle". Which is the original name of current `ThrottlePool`.

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

type FuzzyFn = Option<fn(Duration) -> Duration>;

/// A [`Throttle`] pool to restrict the resource access speed for multiple resources.
///
/// > The generic type `K` is the type of `id`.
///
/// See module document for more detail.
pub struct ThrottlePool<K: Hash + Eq> {
    throttles: Mutex<HashMap<K, Arc<Throttle>>>,
    interval: Duration,
    concurrent: u32,
    fuzzy_fn: FuzzyFn,
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
                    Arc::new(
                        Throttle::builder()
                            .interval(self.interval)
                            .concurrent(self.concurrent)
                            .fuzzy_fn(self.fuzzy_fn)
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

    /// Run some function in particular throttle & retrying if function return `Err(T)`.
    ///
    /// This operation may block current thread by throttle current state and configuration.
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
    fuzzy_fn: FuzzyFn,
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

    /// Set fuzzy_fn to modify `interval` each run, default value is `None`.
    pub fn fuzzy_fn(&mut self, fuzzy_fn: FuzzyFn) -> &mut Self {
        self.fuzzy_fn = fuzzy_fn;
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

    fuzzy_fn: FuzzyFn,
}

impl Throttle {
    pub fn builder() -> ThrottleBuilder {
        ThrottleBuilder::new()
    }

    /// Run some function.
    ///
    /// Call `run(...)` may block current thread by throttle current state and configuration.
    pub fn run<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        // occupying single concurrency quota
        let _semaphore_guard = self.semaphore.access();

        self.waiting();

        f()
    }

    /// Run some function & retrying if function return `Err(T)`.
    ///
    /// Call `run_retry(...)` may block current thread by throttle current state and configuration.
    /// When retrying, the occupied concurrent quota will not release in the half way.
    /// It will continue occupying until the job fully failed or success finally.
    ///
    /// The `rseq` is a "retry delay iterator" which control how many times it can
    /// retry and each retry's delay duration.
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
    ///                 It will not add normal interval between two of retries and throttle
    ///                 will also not interrupt the retry sequence by other pending jobs.
    ///
    /// time pass ----->
    /// ```
    ///
    /// If want to let `Throttle` obey interval for each retry, just re-run `throttle.run(op)`
    /// multiple time by youself.
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
    fuzzy_fn: FuzzyFn,
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

    /// Set fuzzy_fn to modify `interval` each run, default value is `None`.
    pub fn fuzzy_fn(&mut self, fuzzy_fn: FuzzyFn) -> &mut Self {
        self.fuzzy_fn = fuzzy_fn;
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

    /// Generate a new [`Duration`] between `0` ~ `2 * orig_duration` by uniform distribution.
    pub fn uniform(duration: Duration) -> Duration {
        let jitter = random::<f64>() * 2.0;
        let secs = ((duration.as_secs() as f64) * jitter).ceil() as u64;
        let nanos = ((f64::from(duration.subsec_nanos())) * jitter).ceil() as u32;

        Duration::new(secs, nanos)
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
    #[cfg(any(
        target_pointer_width = "8",
        target_pointer_width = "16",
        target_pointer_width = "32",
    ))]
    fn throttle_with_concurrent_large_than_isize_max() {
        assert!(Throttle::builder()
            // If isize::MAX > u32 (mean target_pointer_width = 64 or larger), just
            // cannot compile due to overflow.
            //
            // It's hard to test on 64 bit platform
            .concurrent(isize::MAX as u32 + 1)
            .build()
            .is_none());
    }

    #[test]
    fn throttle_with_fuzzy_fn() {
        Throttle::builder()
            .fuzzy_fn(Some(|_| Duration::default()))
            .build();

        #[cfg(feature = "fuzzy_fns")]
        Throttle::builder()
            .fuzzy_fn(Some(fuzzy_fns::uniform))
            .build();
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
