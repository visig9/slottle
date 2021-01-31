use std::time::Duration;

use super::Interval;

/// Create [fibonacci] delay interval base on failure count.
///
/// [fibonacci]: https://en.wikipedia.org/wiki/Fibonacci_number
///
/// Give `base` as an "unit" duration and `max` as maximum duration. This function
/// will calculate all available fibonacci sequence items (without first 2 item `[0, 1]`).
/// The fibonacci sequence size will also be used for the log size of throttle.
///
/// # Algorithm
///
/// ```ignore
/// // let base = Duration::from_millis(10);    // for example
/// // let max = Duration::from_millis(60);     // for example
///
/// // let max_ratio = 6;                       // == max / base
/// // let fibo_seq = [1, 2, 3, 5];             // last item always <= max_ratio
///
/// // assert_eq!(throttle_log.size(), fibo_seq.len());
///
/// let how_long_should_delay = fibo_seq[throttle_log.failure_count()] * base;
/// ```
///
/// # Panic
///
/// This function will panic when `max < base` or `base == Duration::new(0, 0)`.
pub fn fibonacci(base: Duration, max: Duration) -> Interval {
    /// Generate unit fibonacci sequence
    fn get_unit_fibonacci_seq(max_ratio: u32) -> Vec<u32> {
        let mut prev = 1;
        let mut curr = 1;
        let mut ans = vec![curr];
        loop {
            let new_value = prev + curr;

            if new_value <= max_ratio {
                ans.push(new_value);
                prev = curr;
                curr = new_value;
            } else {
                break ans;
            }
        }
    }

    if base == Duration::new(0, 0) {
        panic!("base duration can not set to zero");
    }

    if max < base {
        panic!("max duration must greater than or equal with base duration");
    }

    // max_ratio always >= 1
    let max_ratio = (max.as_secs_f64() / base.as_secs_f64()) as u32;

    let unit_fibonacci_seq = get_unit_fibonacci_seq(max_ratio);
    let len = unit_fibonacci_seq.len();

    println!("{:#?}", unit_fibonacci_seq);

    Interval::new(
        move |log| {
            let failure_count = log.unwrap().failure_count();

            if (0..len).contains(&failure_count) {
                base * unit_fibonacci_seq[failure_count]
            } else {
                base * *unit_fibonacci_seq.last().unwrap_or(&0)
            }
        },
        len,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::throttle::*;

    #[test]
    #[should_panic]
    fn test_fibonacci_max_smaller_than_base() {
        fibonacci(Duration::from_millis(10), Duration::from_millis(5));
    }

    #[test]
    fn test_fibonacci() {
        fn fibonacci_test_case(base_ms: u64, max_ms: u64, delay_ms: Vec<u64>) {
            fn get_sample_throttle_log(failure_count: usize, total: usize) -> ThrottleLog {
                let mut log = ThrottleLog::new(total);
                for _ in 0..failure_count {
                    log.push(LogRecord {
                        time: Instant::now(),
                        successful: false,
                    })
                }
                log
            }

            let algo = fibonacci(
                Duration::from_millis(base_ms),
                Duration::from_millis(max_ms),
            );
            for (f_count, ms) in delay_ms.iter().enumerate() {
                let throttle_log = get_sample_throttle_log(f_count, algo.log_size);
                assert_eq!(
                    (algo.interval_fn)(Some(&throttle_log)),
                    Duration::from_millis(*ms)
                );
            }
        }

        // max == base
        fibonacci_test_case(10, 10, vec![10, 10, 10]);

        // max > base but not reach next level
        fibonacci_test_case(10, 15, vec![10, 10, 10]);

        // max > base and reach next level
        fibonacci_test_case(10, 20, vec![10, 20, 20, 20]);

        // max > base but not equal the last fibonacci number
        fibonacci_test_case(10, 60, vec![10, 20, 30, 50, 50, 50]);
    }
}
