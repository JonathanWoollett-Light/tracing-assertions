# tracing-assertions

[![Crates.io](https://img.shields.io/crates/v/tracing-assertions)](https://crates.io/crates/tracing-assertions)
[![docs](https://img.shields.io/crates/v/tracing-assertions?color=yellow&label=docs)](https://docs.rs/tracing-assertions)
[![codecov](https://codecov.io/gh/JonathanWoollett-Light/tracing-assertions/branch/master/graph/badge.svg?token=II1xtnbCDX)](https://codecov.io/gh/JonathanWoollett-Light/tracing-assertions)

An assertions framework for [tracing](https://docs.rs/tracing/latest/tracing/).

Simpler and faster than the alternatives.

```rust
use tracing_subscriber::layer::SubscriberExt;
// Initialize a subscriber with the layer.
let asserter = tracing_assertions::Layer::default();
let registry = tracing_subscriber::Registry::default();
let subscriber = registry.with(asserter.clone());
let guard = tracing::subscriber::set_default(subscriber);
let one = asserter.matches("one");
let two = asserter.matches("two");
let and = &one & &two;
tracing::info!("one");
one.assert();
tracing::info!("two");
two.assert();
and.assert();

drop(guard); // Drop `subscriber` as the current subscriber.
```

### Similar crates
- [test-log](https://crates.io/crates/test-log): A replacement of the `#[test]` attribute that initializes logging and/or tracing infrastructure before running tests.
- [tracing_test](https://crates.io/crates/tracing-test): Helper functions and macros that allow for easier testing of crates that use `tracing`.
- [tracing-fluent-assertions](https://crates.io/crates/tracing-fluent-assertions): An fluent assertions framework for tracing.
