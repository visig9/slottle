# Changelog

## [Unreleased]



## [0.4.1] - 2021-03-07

### Fixed

- `ThrottlePool`: `ThrottlePool` not pass configurations to `Throttle` correctly.



## [0.4.0] - 2021-02-11

### Changed

- [BREAKING]: Rename `Algorithm` to `Interval` for easier to understand.
- [BREAKING]: Rename `run_failable` to `run_fallible`.
- Make `ThrottleBuilder::build()` & `ThrottlePoolBuilder::build()` use `&self` rather than `&mut self`.



## [0.3.0] - 2021-01-03

### Removed

- [BREAKING]: Remove `ThrottlePool::run()` & `ThrottlePool::run_retry()`.
    - Use `pool.get(id).run(...)` as replacement.
- [BREAKING]: Remove `ThrottleBuilder::fuzzy_fn()` & `ThrottlePoolBuilder::fuzzy_fn()`.
    - Use `interval(|| ...)` as replacement.
- [BREAKING]: Remove optional features `retrying` & `fuzzy_fn` due to no longer needed.

### Added

- New method `ThrottlePool::get()` to access underlaying `Throttle` instance.
- Method `ThrottleBuilder::interval()` & `ThrottlePoolBuilder::interval()` API now
  allow to set dynamic interval.
- New method `Throttle::run_failable()`.
- Pre-defined interval algorithm `fibonacci()`.

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
