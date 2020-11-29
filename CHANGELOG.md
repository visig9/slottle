# Changelog

## [Unreleased]



## [0.2.0] - 2020-11-29

### Changed

- [BREAKING]: `Throttle::fuzzy_fn()` & `ThrottlePool::fuzzy_fn()` now accept
  `fn(Duration) -> Duration` rather than original `Option<fn(Duration) -> Duration>`.
- [BREAKING]: set `fuzzy_fns` feature as optional.

### Fixed

- `fuzzy_fns::uniform`: let return value include sub-second parts even original
  interval without any sub-second parts (e.g., `Duration::from_secs(10)`).



## [0.1.1] - 2020-11-28

### Fixed

- Try to fix the problem of optional features not build in docs.rs.



## [0.1.0] - 2020-11-28

This is initial release.
