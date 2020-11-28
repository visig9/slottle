# slottle

This is a simple [rust] library provide thread-based throttle pool. It can dynamic create multiple throttles by user given "resource id".

For example, a web scraping tool may treat domain name as resource id to control access speed of each hosts in generic way. User can create multiple pools at the same time, each one have different configurations for different situations.

[rust]: https://www.rust-lang.org/



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
    // make sure we have enough of threads can be blocked:
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
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

    // NOTE: according previous config, expected access speed = 2 per 10ms = 1 per 5ms (in each throttle)

    let startd_time = Instant::now();

    let results: Vec<i32> = vec![1, 2, 3, 4, 5, 6] // total 6 item in here!
        .into_par_iter() // run parallel
        .map(|x| {
            throttles.run(
                x >= 5, // 5,6 in throttle id == `true` & 1,2,3,4 in throttle id == `false`
                // here is the operation we want to throttling
                || {
                    println!(
                        "job {} started, time passed: {}s",
                        x,
                        startd_time.elapsed().as_secs_f64()
                    );
                    x + 1
                },
            )
        })
        .collect();

    assert_eq!(results, vec![2, 3, 4, 5, 6, 7]);
}
````

Output:

```
job 1 started, time passed: 0.000016512s
job 5 started, time passed: 0.000032532s
job 3 started, time passed: 0.005074456s
job 6 started, time passed: 0.005087911s
job 4 started, time passed: 0.010070142s
job 2 started, time passed: 0.015075837s
```

Please check online documents for more detail.



## License

MIT
