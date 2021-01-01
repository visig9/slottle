# Changelog

## [Unreleased]

### Removed

- [BREAKING]: Remove `ThrottlePool::run()` & `ThrottlePool::run_retry()`.
    - Use `pool.get(id).run(...)` as replacement.
- [BREAKING]: Remove `ThrottleBuilder::fuzzy_fn()` & `ThrottlePoolBuilder::fuzzy_fn()`.
    - Use `interval_fn(|| ...)` as replacement.
- [BREAKING]: Remove optional features `retrying` & `fuzzy_fn` due to no longer needed.
- [BREAKING]: Stop re-export helper utilities from `retry` crate.

### Added

- New method `ThrottlePool::get()` to access onderlaying `Throttle` instance.
- New methods `ThrottleBuilder::interval_fn()` & `ThrottlePoolBuilder::interval_fn()` API accept
  an user's algorithm to generate dynamic intervals.
- New method `Throttle::run_failable()` allow throttle detect operation failed
  and may change future interval calculation by `interval_fn()`.
    - This error detecting logic also apply to new method `Throttle::retry()`.

### Changed

- [BREAKING]: New `Throttle::retry()` API replace old `Throttle::run_retry()`.
    - Compare with old version, new API shared delay intervals with all operations
      in current throttle scope.



## [0.2.0] - 2020-11-29

### Changed

- [BREAKING]: `Throttle::fuzzy_fn()` & `ThrottlePool::fuzzy_fn()` now accept
  `fn(Duration) -> Duration` rather than original `Option<fn(Duration) -> Duration>`.
- [BREAKING]: set `fuzzy_fns` feature optional.

### Fixed

- `fuzzy_fns::uniform`: let return value include sub-second parts even original
  interval without any sub-second parts (e.g., `Duration::from_secs(10)`).



## [0.1.1] - 2020-11-28

### Fixed

- Try to fix the problem of optional features not build in docs.rs.



## [0.1.0] - 2020-11-28

This is initial release.
