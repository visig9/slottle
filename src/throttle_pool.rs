use std::{
    collections::HashMap,
    fmt::{self, Debug},
    hash::Hash,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use crate::throttle::{Interval, Throttle, ThrottleBuilder};

#[cfg(feature = "retrying")]
use crate::retrying;

/// A [`Throttle`] pool to restrict the resource access speed for multiple resources.
///
/// See [module](crate) document for more detail.
pub struct ThrottlePool<K: Hash + Eq> {
    throttles: Mutex<HashMap<K, Arc<Throttle>>>,
    throttle_builder: ThrottleBuilder,
}

impl<K: Hash + Eq> ThrottlePool<K> {
    /// Start to create a `ThrottlePool` by [`ThrottlePoolBuilder`].
    pub fn builder() -> ThrottlePoolBuilder<K> {
        ThrottlePoolBuilder::default()
    }

    /// Get a throttle from pool, if not exists, create it.
    pub fn get(&self, id: K) -> Arc<Throttle> {
        Arc::clone(
            self.throttles
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .entry(id)
                .or_insert_with(|| {
                    Arc::new(
                        self.throttle_builder
                            .build()
                            .expect("`concurrent` already varified when ThrottlePool created"),
                    )
                }),
        )
    }
}

impl<K: Hash + Eq> Debug for ThrottlePool<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(&format!("ThrottlePool<{}>", std::any::type_name::<K>(),))
            .field("throttle_builder", &self.throttle_builder)
            .finish()
    }
}

/// Use to build a [`ThrottlePool`].
///
/// Created by [`ThrottlePool::builder()`] API.
pub struct ThrottlePoolBuilder<K: Hash + Eq> {
    concurrent: u32,
    interval: Interval,
    phantom: PhantomData<K>,
}

impl<K: Hash + Eq> Default for ThrottlePoolBuilder<K> {
    fn default() -> Self {
        Self {
            interval: Interval::default(),
            concurrent: 1,
            phantom: PhantomData,
        }
    }
}

impl<K: Hash + Eq> ThrottlePoolBuilder<K> {
    /// Set interval of throttles in this pool.
    ///
    /// The default value is no delay (`Duration::new(0, 0)`)
    ///
    /// # Example
    ///
    /// ```
    /// use slottle::{ThrottlePool, Interval};
    /// use std::time::Duration;
    /// use rand;
    ///
    /// // fixed interval: 10ms
    /// let pool: ThrottlePool<bool> = ThrottlePool::builder()
    ///     .interval(Duration::from_millis(10))
    ///     .build().unwrap();
    ///
    /// // random interval between: 10ms ~ 0ms
    /// let pool: ThrottlePool<bool> = ThrottlePool::builder()
    ///     .interval(|| Duration::from_millis(10).mul_f64(rand::random()))
    ///     .build().unwrap();
    ///
    /// // increasing delay if failed continuously
    /// let pool: ThrottlePool<bool> = ThrottlePool::builder()
    ///     .interval(Interval::new(
    ///         |log| match log.unwrap().failure_count_cont() {
    ///             0 => Duration::from_millis(10),
    ///             1 => Duration::from_millis(30),
    ///             2 => Duration::from_millis(50),
    ///             3 => Duration::from_millis(70),
    ///             _ => unreachable!(),
    ///         },
    ///         3,  // maximum log size
    ///     ))
    ///     .build().unwrap();
    ///
    /// // use pre-defined interval algorithm
    /// let pool: ThrottlePool<bool> = ThrottlePool::builder()
    ///     .interval(slottle::fibonacci(
    ///         Duration::from_millis(10),
    ///         Duration::from_secs(2),
    ///     ))
    ///     .build().unwrap();
    /// ```
    pub fn interval<I>(&mut self, interval: I) -> &mut Self
    where
        I: Into<Interval>,
    {
        self.interval = interval.into();
        self
    }

    /// Set concurrent, default value is `1`.
    pub fn concurrent(&mut self, concurrent: u32) -> &mut Self {
        self.concurrent = concurrent;
        self
    }

    /// Create a new [`ThrottlePool`] with current configuration.
    ///
    /// Return `None` if `concurrent` == `0` or larger than `isize::MAX`.
    pub fn build(&self) -> Option<ThrottlePool<K>> {
        let mut throttle_builder = Throttle::builder();
        throttle_builder.interval(self.interval.clone());
        throttle_builder.concurrent(self.concurrent);

        // check the configurations can initialize throttle properly.
        throttle_builder.build()?;

        Some(ThrottlePool {
            throttles: Mutex::new(HashMap::new()),
            throttle_builder: throttle_builder,
        })
    }
}

impl<K: Hash + Eq> Debug for ThrottlePoolBuilder<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(&format!(
            "ThrottlePoolBuilder<{}>",
            std::any::type_name::<K>()
        ))
        .field("concurrent", &self.concurrent)
        .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;
    use std::time::Duration;

    #[test]
    fn run() {
        let throttles: ThrottlePool<u32> = ThrottlePool::builder()
            .interval(Duration::from_millis(1))
            .concurrent(2)
            .build()
            .unwrap();

        let results: Vec<i32> = vec![1, 2, 3]
            .into_par_iter()
            .map(|x| throttles.get(1).run(|| x + 1))
            .collect();

        assert!(results == vec![2, 3, 4]);
    }

    #[test]
    #[cfg(feature = "retrying")]
    fn run_retry() {
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
                    [Duration::from_millis(10)].iter().cycle().cloned().take(5),
                )
            })
            .collect::<Result<Vec<i32>, i32>>()
            .unwrap();

        assert!(results == vec![2, 3, 4]);
    }
}
