# Changelog

## [Unreleased]

### Changed

- [BREAKING]: `Throttle::fuzzy_fn()` & `ThrottlePool::fuzzy_fn()` now accept
  `fn(Duration) -> Duration` rather than original `Option<fn(Duration) -> Duration>`.
- [BREAKING]: set `fuzzy_fns` feature as optional.



## [0.1.1] - 2020-11-28

### Fixed

- Try to fix the problem of optional features not build in docs.rs.



## [0.1.0] - 2020-11-28

This is initial release.
