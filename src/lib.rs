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
//! // Make sure we have enough of threads can be blocked.
//! // Here we use rayon as example but you can choice any thread implementation.
//! rayon::ThreadPoolBuilder::new()
//!     .num_threads(8)
//!     .build_global()
//!     .unwrap();
//!
//! // Create ThrottlePool.
//! //
//! // In here `id` is `bool` type for demonstration.
//! // If you're writing a web spider, type of `id` might should be `url::Host`.
//! let throttles: ThrottlePool<bool> = ThrottlePool::builder()
//!     .interval(Duration::from_millis(20)) // set interval to 20ms
//!     .concurrent(2) // set concurrent to 2
//!     .build()
//!     .unwrap();
//!
//! // HINT: according previous config, expected access speed is
//! // 2 per 20ms = 1 per 10ms (in each throttle)
//!
//! let started_time = Instant::now();
//!
//! let mut all_added_one: Vec<i32> = vec![1, 2, 3, 4, 5, 6]
//!     .into_par_iter()
//!     .map(|x| {
//!         throttles
//!             .get(x >= 5)    // 5,6 in throttle `true` & 1,2,3,4 in throttle `false`
//!             .run(|| {       // here is the operation we want to throttling
//!                 let time_passed_ms = started_time.elapsed().as_secs_f64() * 1000.0;
//!                 println!(
//!                     "[throttle: {:>5}] allowed job {} to start at: {:.2}ms",
//!                     x >= 5, x, time_passed_ms,
//!                 );
//!
//!                 // // you can add some long-running task to see how throttle work
//!                 // std::thread::sleep(Duration::from_millis(40));
//!
//!                 x + 1
//!             })
//!     })
//!     .collect();
//!
//! assert_eq!(all_added_one, vec![2, 3, 4, 5, 6, 7]);
//! ```
//!
//! Output:
//!
//! ```text
//! [throttle: false] allowed job 1 to start at: 0.09ms
//! [throttle:  true] allowed job 6 to start at: 0.10ms
//! [throttle: false] allowed job 4 to start at: 10.40ms
//! [throttle:  true] allowed job 5 to start at: 10.42ms
//! [throttle: false] allowed job 3 to start at: 20.12ms
//! [throttle: false] allowed job 2 to start at: 30.12ms
//! ```
//!
//!
//!
//! # Crate Naming
//!
//! Crate name `slottle` is the abbr of "slotted throttle". Which is the original name of `ThrottlePool`.

mod throttle;
mod throttle_pool;

#[doc(inline)]
pub use throttle::{RetryableResult, Throttle, ThrottleBuilder, ThrottleLog};

#[doc(inline)]
pub use throttle_pool::{ThrottlePool, ThrottlePoolBuilder};
