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
//! one.assert();
//! tracing::info!("two");
//! two.assert();
//! and.assert();
//!
//! drop(guard); // Drop `subscriber` as the current subscriber.
//! # }
//! ```
//!
//! When failing e.g.
//! ```should_panic
//! use tracing_subscriber::layer::SubscriberExt;
//! # fn main() {
//! let asserter = tracing_assertions::Layer::default();
//! let registry = tracing_subscriber::Registry::default();
//! let subscriber = registry.with(asserter.clone());
//! let guard = tracing::subscriber::set_default(subscriber);
//! let one = asserter.matches("one");
//! let two = asserter.matches("two");
//! let and = &one & &two;
//! tracing::info!("one");
//! and.assert();
//! drop(guard);
//! # }
//! ```
//! Outputs:
//! <pre>
//! thread 'main' panicked at src/lib.rs:14:5:
//! (<font color="green">"one"</font> && <font color="red">"two"</font>)
//! </pre>

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
    pass_all: Arc<AtomicBool>,
    assertions: Arc<Mutex<Vec<Arc<InnerAssertion>>>>,
}
impl Layer {
    /// Creates a string matching assertion.
    ///
    /// # Panics
    ///
    /// When the internal mutex is poisoned.
    pub fn matches(&self, s: impl Into<String>) -> Assertion {
        let inner_assertion = Arc::new(InnerAssertion {
            boolean: AtomicBool::new(false),
            assertion_type: AssertionType::Matches(s.into()),
        });
        self.assertions
            .lock()
            .unwrap()
            .push(inner_assertion.clone());
        Assertion::One(self.pass_all.clone(), inner_assertion)
    }
    /// The inverse of [`Layer::disable`].
    pub fn enable(&self) {
        self.pass_all.store(false, SeqCst);
    }
    /// Tells all assertions to pass.
    ///
    /// Useful when you want to disables certain tested logs in a
    /// test for debugging without needing to comment out all the
    /// assertions you added.
    pub fn disable(&self) {
        self.pass_all.store(true, SeqCst);
    }
}

#[derive(Debug)]
enum AssertionType {
    Matches(String),
}

impl std::fmt::Display for AssertionType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Matches(matches) => write!(f, "{matches}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Assertion {
    And(Box<Assertion>, Box<Assertion>),
    Or(Box<Assertion>, Box<Assertion>),
    One(Arc<AtomicBool>, Arc<InnerAssertion>),
    Not(Box<Assertion>),
}
impl Assertion {
    /// Evaluates the assertion.
    ///
    /// # Panics
    ///
    /// When the assertion is false.
    #[track_caller]
    pub fn assert(&self) {
        assert!(bool::from(self), "{}", self.ansi());
    }
    fn ansi(&self) -> String {
        match self {
            Assertion::One(pass_all, x) => {
                let is_true = if pass_all.load(SeqCst) {
                    true
                } else {
                    x.boolean.load(std::sync::atomic::Ordering::SeqCst)
                };
                let str = format!("{:?}", x.assertion_type.to_string());
                let out = if is_true {
                    ansi_term::Colour::Green.paint(str)
                } else {
                    ansi_term::Colour::Red.paint(str)
                };
                out.to_string()
            }
            Assertion::And(lhs, rhs) => format!("({} && {})", lhs.ansi(), rhs.ansi()),
            Assertion::Or(lhs, rhs) => format!("({} || {})", lhs.ansi(), rhs.ansi()),
            Assertion::Not(not) => format!("!{}", not.ansi()),
        }
    }
}

impl std::ops::Not for Assertion {
    type Output = Self;
    fn not(self) -> Self::Output {
        !&self
    }
}
impl std::ops::Not for &Assertion {
    type Output = Assertion;
    fn not(self) -> Self::Output {
        Assertion::Not(Box::new(self.clone()))
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
            Assertion::One(pass_all, x) => {
                if pass_all.load(SeqCst) {
                    return true;
                }
                x.boolean.load(std::sync::atomic::Ordering::SeqCst)
            }
            Assertion::And(lhs, rhs) => bool::from(&**lhs) && bool::from(&**rhs),
            Assertion::Or(lhs, rhs) => bool::from(&**lhs) || bool::from(&**rhs),
            Assertion::Not(not) => !bool::from(&**not),
        }
    }
}
impl From<Assertion> for bool {
    fn from(value: Assertion) -> Self {
        bool::from(&value)
    }
}

#[derive(Debug)]
pub struct InnerAssertion {
    boolean: AtomicBool,
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
    fn pass_all() {
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);

        info!("stuff");
        let condition = asserter.matches("missing");
        asserter.disable();
        info!("more stuff");
        condition.assert();
        asserter.enable();
        (!condition).assert();

        drop(guard);
    }

    #[test]
    #[should_panic(
        expected = "((\u{1b}[32m\"one\"\u{1b}[0m && \u{1b}[31m\"two\"\u{1b}[0m) || (\u{1b}[31m\"three\"\u{1b}[0m && !\u{1b}[31m\"four\"\u{1b}[0m))"
    )]
    fn panics() {
        let asserter = Layer::default();
        let registry = Registry::default();
        let subscriber = registry.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        let one = asserter.matches("one");
        let two = asserter.matches("two");
        let three = asserter.matches("three");
        let four = asserter.matches("four");
        let assertion = one & two | three & !four;
        info!("one");
        asserter.disable();
        assertion.assert();
        assert_eq!(assertion.ansi(),"((\u{1b}[32m\"one\"\u{1b}[0m && \u{1b}[32m\"two\"\u{1b}[0m) || (\u{1b}[32m\"three\"\u{1b}[0m && !\u{1b}[32m\"four\"\u{1b}[0m))");
        asserter.enable();
        assertion.assert();
        drop(guard);
    }

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
        (!&two).assert();

        info!("one");

        // Still false.
        (!&two).assert();
        (!&or).assert();
        (!&or2).assert();

        info!("two");

        // The assertion is true as a message matching `two` has been encountered.
        two.assert();
        or.assert();
        or2.assert();
        (!&and).assert();
        (!&and2).assert();

        info!("three");

        // Still true.
        two.assert();
        and.assert();
        and2.assert();

        // If an assertion is created after the message, it will be false.
        // Each assertion can only be fulfilled based on messages after its creation.
        let two = asserter.matches("two");
        (!&two).assert();
        assert!(!bool::from(two));

        drop(guard);
    }
}
