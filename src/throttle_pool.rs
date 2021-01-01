use std::{
    collections::HashMap,
    fmt::{self, Debug},
    hash::Hash,
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::throttle::{IntervalFn, Throttle, ThrottleLog};

#[cfg(feature = "retrying")]
use crate::retrying;

/// A [`Throttle`] pool to restrict the resource access speed for multiple resources.
///
/// See [module](crate) document for more detail.
pub struct ThrottlePool<K: Hash + Eq> {
    throttles: Mutex<HashMap<K, Arc<Throttle>>>,
    interval_fn: Arc<IntervalFn>,
    concurrent: u32,
    log_size: usize,
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
                        Throttle::builder()
                            .interval_fn_from_arc(&self.interval_fn)
                            .concurrent(self.concurrent)
                            .enable_log(self.log_size)
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
            .field("log_size", &self.log_size)
            .field("concurrent", &self.concurrent)
            .finish()
    }
}

/// Use to build a [`ThrottlePool`].
///
/// Created by [`ThrottlePool::builder()`] API.
pub struct ThrottlePoolBuilder<K: Hash + Eq> {
    interval_fn: Arc<IntervalFn>,
    concurrent: u32,
    log_size: usize,
    phantom: PhantomData<K>,
}

impl<K: Hash + Eq> Default for ThrottlePoolBuilder<K> {
    fn default() -> Self {
        Self {
            interval_fn: Arc::new(|_| Duration::default()),
            concurrent: 1,
            log_size: 0,
            phantom: PhantomData,
        }
    }
}

impl<K: Hash + Eq> ThrottlePoolBuilder<K> {
    /// Set interval as a fixed value.
    ///
    /// This function just a shortcut of [`interval_fn(|| interval)`](Self::interval_fn).
    pub fn interval(&mut self, interval: Duration) -> &mut Self {
        self.interval_fn(move |_| interval);
        self
    }

    /// Set interval as a dynamic value.
    ///
    /// This function allow user calculate dynamic interval by statistic
    /// data from [`ThrottleLog`].
    ///
    /// The default value is `|_| Duration::default()` (no delay).
    pub fn interval_fn<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(Option<&ThrottleLog>) -> Duration + Send + Sync + 'static,
    {
        self.interval_fn = Arc::new(f);
        self
    }

    /// Set [`ThrottleLog`] maximum size.
    ///
    /// The default value is `0`.
    pub fn log_size(&mut self, size: usize) -> &mut Self {
        self.log_size = size;
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
    pub fn build(&mut self) -> Option<ThrottlePool<K>> {
        // check the configurations can initialize throttle properly.
        Throttle::builder()
            .interval_fn_from_arc(&self.interval_fn)
            .concurrent(self.concurrent)
            .enable_log(self.log_size)
            .build()?;

        Some(ThrottlePool {
            throttles: Mutex::new(HashMap::new()),
            interval_fn: Arc::clone(&self.interval_fn),
            log_size: self.log_size,
            concurrent: self.concurrent,
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
        .field("log_size", &self.log_size)
        .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

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
