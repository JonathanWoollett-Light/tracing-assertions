//! An assertions framework for [tracing](https://docs.rs/tracing/latest/tracing/).
//!
//! Simpler and faster than the alternatives.
//!
//! - **Why use this instead of [tracing_test](https://docs.rs/tracing-test/latest/tracing_test/)?** Typical assertions make use of `lines.iter()` checking every line previously logged.
//! - **Why use this instead of [tracing_fluent_assertions](https://docs.rs/tracing-fluent-assertions/latest/tracing_fluent_assertions/)?** Works with [`Event`](https://docs.rs/tracing/latest/tracing/struct.Event.html)s.
//!
//! ```
//! use tracing_subscriber::layer::SubscriberExt;
//! # fn main() {
//! // Initialize a subscriber with the layer.
//! let asserter = tracing_assertions::Layer::default();
//! let registry = tracing_subscriber::Registry::default();
//! let subscriber = registry.with(asserter.clone());
//! let guard = tracing::subscriber::set_default(subscriber);
//! let assertion = asserter.matches("two"); // Make assertion.
//! tracing::info!("two"); // Send event.
//! assert!(assertion); // Check assertion.
//!
//! drop(guard); // Drop `subscriber` as the current subscriber.
//! # }
//! ```

#![warn(clippy::pedantic)]

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::field::Field;
use tracing::Event;
use tracing::Subscriber;
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::Context;

#[derive(Default, Clone)]
pub struct Layer {
    assertions: Arc<Mutex<Vec<InnerAssertion>>>,
}
impl Layer {
    /// Creates a string matching assertion.
    ///
    /// # Panics
    ///
    /// When the internal mutex is poisoned.
    pub fn matches(&self, s: impl Into<String>) -> Assertion {
        let boolean = Arc::new(AtomicBool::new(false));
        self.assertions.lock().unwrap().push(InnerAssertion {
            boolean: boolean.clone(),
            assertion_type: AssertionType::Matches(s.into()),
        });
        Assertion(boolean)
    }
}

enum AssertionType {
    Matches(String),
}
#[derive(Debug)]
pub struct Assertion(Arc<AtomicBool>);
impl std::ops::Not for Assertion {
    type Output = bool;
    fn not(self) -> Self::Output {
        !self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
impl std::ops::Not for &Assertion {
    type Output = bool;
    fn not(self) -> Self::Output {
        !self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
struct InnerAssertion {
    boolean: Arc<AtomicBool>,
    assertion_type: AssertionType,
}

struct EventVisitor<'a>(&'a mut String);
impl<'a> Visit for EventVisitor<'a> {
    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        *self.0 = format!("{value:?}");
    }
}

impl<S: Subscriber> tracing_subscriber::layer::Layer<S> for Layer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // TODO This is a stupid way to access the message, surely there is a better way to get the message.
        let mut message = String::new();
        event.record(&mut EventVisitor(&mut message) as &mut dyn Visit);
        let mut assertions = self.assertions.lock().unwrap();
        let mut i = 0;
        while i < assertions.len() {
            dbg!();
            let result = match &assertions[i].assertion_type {
                AssertionType::Matches(expected) => *expected == message,
            };
            assertions[i].boolean.store(result, SeqCst);
            if result {
                assertions.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    #[test]
    fn matches() {
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);

        let assertion = asserter.matches("two");

        // The assertion is false as message matching `two` has not been encountered.
        assert!(!&assertion);

        info!("one");

        // Still false.
        assert!(!&assertion);

        info!("two");

        // The assertion is true as a message matching `two` has been encountered.
        assert!(&assertion);

        info!("three");

        // Still true.
        assert!(assertion);

        // If an assertion is created after the message, it will be false.
        // Each assertion can only be fulfilled based on messages after its creation.
        let assertion = asserter.matches("two");
        assert!(!assertion);

        drop(guard);
    }
}
