# slottle

A simple Rust crate provide thread-based throttle pool. It can dynamic create multiple throttles by user given resource id.

For example, a web scraping tool may treat domain name as resource id to control access speed of each hosts in generic way. User can create multiple pools at the same time, each one have different configurations for different situations.



# Features

- Not just individual throttle but also provide throttle pool. (can ignore if don't need it)
- Both concurrent & delay interval are configurable.
- Allow user defined algorithm to generate delay interval dynamically.
- Failure sensitive & builtin retry support.
- Easy to use.
- Exhaustive document.

Check [online document](https://docs.rs/slottle/) for more detail.



## License

MIT
