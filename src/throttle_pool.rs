use std::{
    collections::HashMap,
    fmt::{self, Debug},
    hash::Hash,
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::FuzzyFn;
use crate::Throttle;

#[cfg(feature = "retrying")]
use crate::retrying;

/// A [`Throttle`] pool to restrict the resource access speed for multiple resources.
///
/// See [module](crate) document for more detail.
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

/// Use to build a [`ThrottlePool`].
///
/// Create by [`ThrottlePool::builder()`] API.
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
            .map(|x| throttles.run(1, || x + 1))
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
