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

use std::time::Duration;

mod throttle;

#[doc(inline)]
pub use throttle::{Throttle, ThrottleBuilder};

mod throttle_pool;

#[doc(inline)]
pub use throttle_pool::{ThrottlePool, ThrottlePoolBuilder};

type FuzzyFn = fn(interval: Duration) -> Duration;

#[cfg(feature = "retrying")]
pub mod retrying {
    //! Re-export utils in [`retry`] crate for `retrying` feature related facility.
    //!
    //! # Note
    //!
    //! Only exists when `retrying` feature on.

    pub use retry::OperationResult;
}

#[cfg(feature = "fuzzy_fns")]
pub mod fuzzy_fns {
    //! Helper functions can assign into [`ThrottleBuilder::fuzzy_fn()`](crate::throttle::ThrottleBuilder::fuzzy_fn()).
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
        use crate::fuzzy_fns;
        use std::time::Duration;

        #[test]
        fn uniform() {
            vec![Duration::from_secs(10); 100]
                .into_iter()
                .map(fuzzy_fns::uniform)
                .for_each(|d| {
                    assert!((Duration::from_secs(0)..Duration::from_secs(20)).contains(dbg!(&d)))
                });
        }
    }
}
