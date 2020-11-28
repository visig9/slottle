# slottle

This is a simple [rust] library provide thread-based throttle pool. It can dynamic create multiple throttles by user given "resource id".

For example, a web scraping tool may treat domain name as resource id to control access speed of each hosts in generic way. User can create multiple pools at the same time, each one have different configurations for different situations.

[rust]: https://www.rust-lang.org/



## Example

```rust
use std::time::Duration;
use slottle::ThrottlePool;
use rayon::prelude::*;

// Create ThrottlePool to store some state.
//
// In here `id` is `bool` type for demonstration. If you're writing
// a web spider, type of `id` might be `url::Host`.
let throttles: ThrottlePool<bool> =
    ThrottlePool::builder()
        .interval(Duration::from_millis(1)) // set interval to 1 ms
        .concurrent(2)                      // 2 concurrent for each throttle
        .build()
        .unwrap();

// make sure you have enough of threads. For example:
rayon::ThreadPoolBuilder::new().num_threads(8).build_global().unwrap();

let results: Vec<i32> = vec![1, 2, 3, 4, 5]
    .into_par_iter()        // run parallel
    .map(|x| throttles.run(
        x == 5,             // 5 in throttle `true`, 1,2,3,4 in throttle `false`
        || {x + 1},         // here is the operation should be throttled
    ))
    .collect();

assert_eq!(results, vec![2, 3, 4, 5, 6,]);
```

Please check online documents for more detail.



## License

MIT
