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
//! let one = asserter.matches("one");
//! let two = asserter.matches("two");
//! let and = &one & &two;
//! tracing::info!("one");
//! assert!(one);
//! tracing::info!("two");
//! assert!(two);
//! assert!(and);
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
        Assertion::One(boolean)
    }
}

enum AssertionType {
    Matches(String),
}

#[derive(Debug, Clone)]
pub enum Assertion {
    And(Box<Assertion>, Box<Assertion>),
    Or(Box<Assertion>, Box<Assertion>),
    One(Arc<AtomicBool>),
}

impl std::ops::Not for Assertion {
    type Output = bool;
    fn not(self) -> Self::Output {
        !&self
    }
}
impl std::ops::Not for &Assertion {
    type Output = bool;
    fn not(self) -> Self::Output {
        !bool::from(self)
    }
}

impl std::ops::BitAnd for Assertion {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        &self & &rhs
    }
}
impl std::ops::BitAnd for &Assertion {
    type Output = Assertion;
    fn bitand(self, rhs: Self) -> Self::Output {
        Assertion::And(Box::new(self.clone()), Box::new(rhs.clone()))
    }
}
impl std::ops::BitOr for Assertion {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        &self | &rhs
    }
}
impl std::ops::BitOr for &Assertion {
    type Output = Assertion;
    fn bitor(self, rhs: Self) -> Self::Output {
        Assertion::Or(Box::new(self.clone()), Box::new(rhs.clone()))
    }
}

impl From<&Assertion> for bool {
    fn from(value: &Assertion) -> Self {
        match value {
            Assertion::One(x) => x.load(std::sync::atomic::Ordering::SeqCst),
            Assertion::And(lhs, rhs) => bool::from(&**lhs) && bool::from(&**rhs),
            Assertion::Or(lhs, rhs) => bool::from(&**lhs) || bool::from(&**rhs),
        }
    }
}
impl From<Assertion> for bool {
    fn from(value: Assertion) -> Self {
        bool::from(&value)
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

        let two = asserter.matches("two");
        let three = asserter.matches("three");
        let or = &two | &three;
        let and = &two & &three;
        let or2 = two.clone() | three.clone();
        let and2 = two.clone() & three.clone();

        // The assertion is false as message matching `two` has not been encountered.
        assert!(!&two);

        info!("one");

        // Still false.
        assert!(!&two);
        assert!(!&or);
        assert!(!&or2);

        info!("two");

        // The assertion is true as a message matching `two` has been encountered.
        assert!(&two);
        assert!(or);
        assert!(or2);
        assert!(!&and);
        assert!(!&and2);

        info!("three");

        // Still true.
        assert!(&two);
        assert!(and);
        assert!(and2);

        // If an assertion is created after the message, it will be false.
        // Each assertion can only be fulfilled based on messages after its creation.
        let two = asserter.matches("two");
        assert!(!&two);
        assert!(!bool::from(two));

        drop(guard);
    }
}
