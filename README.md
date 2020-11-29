# slottle

This is a simple Rust library provide thread-based throttle pool. It can dynamic create multiple throttles by user given resource id.

For example, a web scraping tool may treat domain name as resource id to control access speed of each hosts in generic way. User can create multiple pools at the same time, each one have different configurations for different situations.



## Example

````rust
//! ```cargo
//! [dependencies]
//! rayon = "1.5.0"
//! slottle = "0.1.0"
//! ```

use rayon::prelude::*;
use slottle::ThrottlePool;
use std::time::{Duration, Instant};

fn main() {
    // Make sure we have enough of threads can be blocked.
    // Here we use rayon as example but you can choice any thread implementation.
    rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build_global()
        .unwrap();

    // Create ThrottlePool.
    //
    // In here `id` is `bool` type for demonstration.
    // If you're writing a web spider, type of `id` might should be `url::Host`.
    let throttles: ThrottlePool<bool> = ThrottlePool::builder()
        .interval(Duration::from_millis(10)) // set interval to 10ms
        .concurrent(2) // set concurrent to 2
        .build()
        .unwrap();

    // HINT: according previous config, expected access speed is
    // 2 per 10ms = 1 per 5ms (in each throttle)

    let started_time = Instant::now();

    let mut time_passed_ms_vec: Vec<f64> = vec![1, 2, 3, 4, 5, 6]
        .into_par_iter()
        .map(|x| {
            throttles.run(
                x >= 5, // 5,6 in throttle `true` & 1,2,3,4 in throttle `false`
                // here is the operation we want to throttling
                || {
                    let time_passed_ms = started_time.elapsed().as_secs_f64() * 1000.0;
                    println!(
                        "[throttle: {:>5}] allowed job {} started at: {:.2}ms",
                        x >= 5, x, time_passed_ms,
                    );

                    // // you can also add some long-running task to see how throttle work
                    // std::thread::sleep(Duration::from_millis(20));

                    time_passed_ms
                },
            )
        })
        .collect();

    // Verify all time_passed_ms as we expected...

    time_passed_ms_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let expected_time_passed_ms = [0.0, 0.0, 5.0, 5.0, 10.0, 15.0];
    time_passed_ms_vec
        .into_iter()
        .zip(expected_time_passed_ms.iter())
        .for_each(|(value, expected)| assert_eq_approx("time_passed (ms)", value, *expected, 1.0));
}

/// assert value approximate equal to expected value.
fn assert_eq_approx(name: &str, value: f64, expected: f64, espilon: f64) {
    let expected_range = (expected - espilon)..(expected + espilon);
    assert!(
        expected_range.contains(&value),
        "{}: {} out of accpetable range: {:?}",
        name, value, expected_range,
    );
}
````

Output:

```
[throttle: false] allowed job 1 to start at: 0.05ms
[throttle:  true] allowed job 5 to start at: 0.06ms
[throttle: false] allowed job 2 to start at: 5.10ms
[throttle:  true] allowed job 6 to start at: 5.12ms
[throttle: false] allowed job 3 to start at: 10.11ms
[throttle: false] allowed job 4 to start at: 15.11ms
```

Please check online documents for more detail.



## License

MIT
