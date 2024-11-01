//! [![Crates.io](https://img.shields.io/crates/v/tracing-assertions)](https://crates.io/crates/tracing-assertions)
//! [![docs](https://img.shields.io/crates/v/tracing-assertions?color=yellow&label=docs)](https://docs.rs/tracing-assertions)
//! [![codecov](https://codecov.io/gh/JonathanWoollett-Light/tracing-assertions/branch/master/graph/badge.svg?token=II1xtnbCDX)](https://codecov.io/gh/JonathanWoollett-Light/tracing-assertions)
//!
//! An assertions framework for [tracing](https://docs.rs/tracing/latest/tracing/).
//!
//! Simpler and faster than the alternatives.
//!
//! ```
//! use tracing_subscriber::layer::SubscriberExt;
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
//! ```
//!
//! ### Failing
//!
//! When failing e.g.
//! ```should_panic
//! # use tracing_subscriber::layer::SubscriberExt;
//! # let asserter = tracing_assertions::Layer::default();
//! # let registry = tracing_subscriber::Registry::default();
//! # let subscriber = registry.with(asserter.clone());
//! # let guard = tracing::subscriber::set_default(subscriber);
//! let one = asserter.matches("one");
//! let two = asserter.matches("two");
//! let and = &one & &two;
//! tracing::info!("one");
//! and.assert();
//! # drop(guard);
//! ```
//! Outputs:
//! <pre>
//! thread 'main' panicked at src/lib.rs:14:5:
//! (<font color="green">"one"</font> && <font color="red">"two"</font>)
//! </pre>
//!
//! ### Operations
//!
//! Logical operations clone the underlying assertions.
//! ```
//! # use tracing_subscriber::layer::SubscriberExt;
//! # let asserter = tracing_assertions::Layer::default();
//! # let registry = tracing_subscriber::Registry::default();
//! # let subscriber = registry.with(asserter.clone());
//! # let guard = tracing::subscriber::set_default(subscriber);
//! let one = asserter.matches("one");
//! let two = asserter.matches("two");
//! let and = &one & &two;
//! tracing::info!("one");
//! tracing::info!("two");
//! one.assert().reset();
//! and.assert().reset();
//! two.assert();
//! (!one).assert();
//! (!and).assert();
//! ```
//! Calling [`Assertion::reset`] on `one` does not affect the value of `and` and calling [`Assertion::reset`] on `and` does not affect the value of `two`.
//!
//! ### Similar crates
//! - [test-log](https://crates.io/crates/test-log): A replacement of the `#[test]` attribute that initializes logging and/or tracing infrastructure before running tests.
//! - [tracing_test](https://crates.io/crates/tracing-test): Helper functions and macros that allow for easier testing of crates that use `tracing`.
//! - [tracing-fluent-assertions](https://crates.io/crates/tracing-fluent-assertions): An fluent assertions framework for tracing.
//!

use std::fmt::Debug;
use std::ops::{BitAnd, BitOr};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::field::Field;
use tracing::Event;
use tracing::Subscriber;
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::Context;

#[cfg(feature = "regex")]
use regex::Regex;

/// The assertion layer.
#[derive(Default, Clone, Debug)]
pub struct Layer(Arc<InnerLayer>);

/// The inner layer shared between assertions and the assertion layer.
///
/// You should probably not use this directly.
#[derive(Default, Debug)]
struct InnerLayer {
    pass_all: AtomicBool,
    assertions: Mutex<Vec<Arc<InnerAssertion>>>,
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
        self.0
            .assertions
            .lock()
            .unwrap()
            .push(inner_assertion.clone());
        Assertion(AssertionWrapper::One {
            assertion: inner_assertion.clone(),
            asserter: self.0.clone(),
        })
    }
    /// Creates a string matching assertion on the debug string of a value.
    ///
    /// This exists because
    /// ```
    /// # use tracing_subscriber::{layer::SubscriberExt, Registry};
    /// # #[derive(Debug)]
    /// # struct MyStruct { x: i32, y: i32 }
    /// # let asserter = tracing_assertions::Layer::default();
    /// # let base_subscriber = Registry::default();
    /// # let subscriber = base_subscriber.with(asserter.clone());
    /// # let guard = tracing::subscriber::set_default(subscriber);
    /// let condition = asserter.debug(MyStruct { x: 2, y: 3 });
    /// ```
    /// is more readable than
    /// ```
    /// # use tracing_subscriber::{layer::SubscriberExt, Registry};
    /// # #[derive(Debug)]
    /// # struct MyStruct { x: i32, y: i32 }
    /// # let asserter = tracing_assertions::Layer::default();
    /// # let base_subscriber = Registry::default();
    /// # let subscriber = base_subscriber.with(asserter.clone());
    /// # let guard = tracing::subscriber::set_default(subscriber);
    /// let condition = asserter.matches(format!("{:?}", MyStruct { x: 2, y: 3 }));
    pub fn debug(&self, s: impl Debug) -> Assertion {
        self.matches(format!("{s:?}"))
    }
    /// Creates a regex matching assertion.
    ///
    /// # Errors
    ///
    /// When the conversion to [`Regex`] fails.
    ///
    /// # Panics
    ///
    /// When the internal mutex is poisoned.
    #[cfg(feature = "regex")]
    pub fn regex<T>(&self, s: T) -> Result<Assertion, <Regex as TryFrom<T>>::Error>
    where
        Regex: TryFrom<T>,
    {
        let inner_assertion = Arc::new(InnerAssertion {
            boolean: AtomicBool::new(false),
            assertion_type: AssertionType::Regex(Regex::try_from(s)?),
        });
        self.0
            .assertions
            .lock()
            .unwrap()
            .push(inner_assertion.clone());
        Ok(Assertion(AssertionWrapper::One {
            assertion: inner_assertion.clone(),
            asserter: self.0.clone(),
        }))
    }
    /// The inverse of [`Layer::disable`].
    pub fn enable(&self) {
        self.0.pass_all.store(false, SeqCst);
    }
    /// Tells all assertions to pass.
    ///
    /// Useful when you want to disables certain tested logs in a
    /// test for debugging without needing to comment out all the
    /// assertions you added.
    pub fn disable(&self) {
        self.0.pass_all.store(true, SeqCst);
    }
}

#[derive(Debug, Clone)]
enum AssertionType {
    Matches(String),
    #[cfg(feature = "regex")]
    Regex(Regex),
}

impl std::fmt::Display for AssertionType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use AssertionType::*;
        match self {
            Matches(matches) => write!(f, "{matches}"),
            #[cfg(feature = "regex")]
            Regex(regex) => write!(f, "{regex}"),
        }
    }
}

/// An assertion.
#[derive(Debug, Clone)]
pub struct Assertion(AssertionWrapper);

/// This exists since there is no way of making enum variants private.
#[derive(Debug)]
enum AssertionWrapper {
    And {
        lhs: Box<Assertion>,
        rhs: Box<Assertion>,
    },
    Or {
        lhs: Box<Assertion>,
        rhs: Box<Assertion>,
    },
    One {
        assertion: Arc<InnerAssertion>,
        asserter: Arc<InnerLayer>,
    },
    Not {
        assertion: Box<Assertion>,
    },
}
impl Clone for AssertionWrapper {
    fn clone(&self) -> AssertionWrapper {
        use AssertionWrapper::*;
        match &self {
            One {
                assertion,
                asserter,
            } => {
                let new_assertion = Arc::new(InnerAssertion {
                    boolean: AtomicBool::from(assertion.boolean.load(SeqCst)),
                    assertion_type: assertion.assertion_type.clone(),
                });
                asserter
                    .assertions
                    .lock()
                    .unwrap()
                    .push(new_assertion.clone());
                One {
                    assertion: new_assertion,
                    asserter: asserter.clone(),
                }
            }
            Not { assertion } => Not {
                assertion: assertion.clone(),
            },
            And { lhs, rhs } => And {
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            },
            Or { lhs, rhs } => Or {
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            },
        }
    }
}

impl Assertion {
    /// Evaluates the assertion.
    ///
    /// # Panics
    ///
    /// When the assertion is false.
    #[allow(clippy::must_use_candidate)] // `let _ = x.assert();` is ugly.
    #[track_caller]
    pub fn assert(&self) -> &Self {
        assert!(bool::from(self), "{}", self.ansi());
        self
    }
    /// Create a new assertion with the same condition.
    ///
    /// ```
    /// use tracing_subscriber::layer::SubscriberExt;
    /// let asserter = tracing_assertions::Layer::default();
    /// let registry = tracing_subscriber::Registry::default();
    /// let subscriber = registry.with(asserter.clone());
    /// let guard = tracing::subscriber::set_default(subscriber);
    /// let one = asserter.matches("one");
    /// tracing::info!("one");
    /// one.assert();
    /// let one2 = one.repeat();
    /// (!&one2).assert();
    /// tracing::info!("one");
    /// one2.assert();
    /// ```
    ///
    /// # Panics
    ///
    /// When the inner mutex is poisoned.
    #[must_use]
    pub fn repeat(&self) -> Self {
        use AssertionWrapper::*;
        let inner = match &self.0 {
            One {
                assertion,
                asserter,
            } => {
                let new_assertion = Arc::new(InnerAssertion {
                    boolean: AtomicBool::new(false),
                    assertion_type: assertion.assertion_type.clone(),
                });
                asserter
                    .assertions
                    .lock()
                    .unwrap()
                    .push(new_assertion.clone());
                One {
                    assertion: new_assertion,
                    asserter: asserter.clone(),
                }
            }
            Not { assertion } => Not {
                assertion: Box::new(assertion.repeat()),
            },
            And { lhs, rhs } => And {
                lhs: Box::new(lhs.repeat()),
                rhs: Box::new(rhs.repeat()),
            },
            Or { lhs, rhs } => Or {
                lhs: Box::new(lhs.repeat()),
                rhs: Box::new(rhs.repeat()),
            },
        };
        Self(inner)
    }

    /// Resets the assertion.
    ///
    /// ```
    /// use tracing_subscriber::layer::SubscriberExt;
    /// let asserter = tracing_assertions::Layer::default();
    /// let registry = tracing_subscriber::Registry::default();
    /// let subscriber = registry.with(asserter.clone());
    /// let guard = tracing::subscriber::set_default(subscriber);
    /// let one = asserter.matches("one");
    /// tracing::info!("one");
    /// one.assert().reset();
    /// (!&one).assert();
    /// tracing::info!("one");
    /// one.assert();
    /// ```
    ///
    /// # Panics
    ///
    /// When the inner mutex is poisoned.
    pub fn reset(&self) {
        use AssertionWrapper::*;
        match &self.0 {
            One {
                assertion,
                asserter,
            } => {
                if assertion.boolean.swap(false, SeqCst) {
                    asserter.assertions.lock().unwrap().push(assertion.clone());
                }
            }
            Not { assertion } => assertion.reset(),
            And { lhs, rhs } | Or { lhs, rhs } => {
                lhs.reset();
                rhs.reset();
            }
        }
    }

    fn ansi(&self) -> String {
        use AssertionWrapper::*;

        match &self.0 {
            One {
                assertion,
                asserter,
            } => {
                let is_true = if asserter.pass_all.load(SeqCst) {
                    true
                } else {
                    assertion.boolean.load(std::sync::atomic::Ordering::SeqCst)
                };
                let str = format!("{:?}", assertion.assertion_type.to_string());
                let out = if is_true {
                    ansi_term::Colour::Green.paint(str)
                } else {
                    ansi_term::Colour::Red.paint(str)
                };
                out.to_string()
            }
            And { lhs, rhs } => format!("({} && {})", lhs.ansi(), rhs.ansi()),
            Or { lhs, rhs } => format!("({} || {})", lhs.ansi(), rhs.ansi()),
            Not { assertion } => format!("!{}", assertion.ansi()),
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
        Assertion(AssertionWrapper::Not {
            assertion: Box::new(self.clone()),
        })
    }
}

impl BitAnd for Assertion {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Assertion(AssertionWrapper::And {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitAnd for &Assertion {
    type Output = Assertion;
    fn bitand(self, rhs: Self) -> Self::Output {
        Assertion(AssertionWrapper::And {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitAnd<&Assertion> for Assertion {
    type Output = Assertion;
    fn bitand(self, rhs: &Self) -> Self::Output {
        Assertion(AssertionWrapper::And {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitAnd<Assertion> for &Assertion {
    type Output = Assertion;
    fn bitand(self, rhs: Assertion) -> Self::Output {
        Assertion(AssertionWrapper::And {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitOr for Assertion {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Assertion(AssertionWrapper::Or {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitOr for &Assertion {
    type Output = Assertion;
    fn bitor(self, rhs: Self) -> Self::Output {
        Assertion(AssertionWrapper::Or {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitOr<&Assertion> for Assertion {
    type Output = Self;
    fn bitor(self, rhs: &Assertion) -> Self::Output {
        Assertion(AssertionWrapper::Or {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}
impl BitOr<Assertion> for &Assertion {
    type Output = Assertion;
    fn bitor(self, rhs: Assertion) -> Self::Output {
        Assertion(AssertionWrapper::Or {
            lhs: Box::new(self.clone()),
            rhs: Box::new(rhs.clone()),
        })
    }
}

impl From<&Assertion> for bool {
    fn from(value: &Assertion) -> Self {
        use AssertionWrapper::*;
        match &value.0 {
            One {
                assertion,
                asserter,
            } => {
                if asserter.pass_all.load(SeqCst) {
                    return true;
                }
                assertion.boolean.load(std::sync::atomic::Ordering::SeqCst)
            }
            And { lhs, rhs } => bool::from(&**lhs) && bool::from(&**rhs),
            Or { lhs, rhs } => bool::from(&**lhs) || bool::from(&**rhs),
            Not { assertion } => !bool::from(&**assertion),
        }
    }
}
impl From<Assertion> for bool {
    fn from(value: Assertion) -> Self {
        bool::from(&value)
    }
}

/// The inner assertion shared between assertions and the assertion layer.
///
/// You should probably not use this directly.
#[derive(Debug)]
struct InnerAssertion {
    boolean: AtomicBool,
    assertion_type: AssertionType,
}

struct EventVisitor<'a>(&'a mut String);
impl Visit for EventVisitor<'_> {
    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        *self.0 = format!("{value:?}");
    }
}

impl<S: Subscriber> tracing_subscriber::layer::Layer<S> for Layer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // TODO This is a stupid way to access the message, surely there is a better way to get the message.
        let mut message = String::new();
        event.record(&mut EventVisitor(&mut message) as &mut dyn Visit);
        let mut assertions = self.0.assertions.lock().unwrap();
        let mut i = 0;
        while i < assertions.len() {
            let result = match &assertions[i].assertion_type {
                AssertionType::Matches(expected) => *expected == message,
                AssertionType::Regex(regex) => regex.is_match(&message),
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

    #[cfg(feature = "regex")]
    #[test]
    fn regex_pass() {
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        let condition = asserter.regex("01234.6789").unwrap();
        info!("0123456789");
        condition.assert();
        drop(guard);
    }

    #[cfg(feature = "regex")]
    #[should_panic(expected = "\u{1b}[31m\"01234.789\"\u{1b}[0m")]
    #[test]
    fn regex_fail() {
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        let condition = asserter.regex("01234.789").unwrap();
        info!("0123456789");
        condition.assert();
        drop(guard);
    }

    #[test]
    fn debug() {
        #[allow(dead_code)]
        #[derive(Debug)]
        struct MyStruct {
            x: i32,
            y: i32,
        }
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        let value = MyStruct { x: 2, y: 3 };
        let condition = asserter.debug(&value);
        info!("{value:?}");
        condition.assert();
        drop(guard);
    }

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
    fn and() {
        let asserter = Layer::default();
        let registry = Registry::default();
        let subscriber = registry.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        let one = asserter.matches("one");
        let two = asserter.matches("two");
        let a = &one & two.clone();
        let b = one.clone() & &two;
        let c = &one & &two;
        let d = one & two;
        info!("one");
        info!("two");
        a.assert();
        b.assert();
        c.assert();
        d.assert();
        drop(guard);
    }

    #[test]
    fn or() {
        let asserter = Layer::default();
        let registry = Registry::default();
        let subscriber = registry.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);
        let one = asserter.matches("one");
        let two = asserter.matches("two");
        let a = &one | two.clone();
        let b = one.clone() | &two;
        let c = &one | &two;
        let d = one | two;
        info!("one");
        a.assert();
        b.assert();
        c.assert();
        d.assert();
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

    #[test]
    fn repeat() {
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);

        let one = asserter.matches("one");
        let two = asserter.matches("two");
        let or = &one | &two;
        let and = &one & &two;
        let not = !&one;

        info!("one");
        info!("two");

        one.assert();
        two.assert();
        or.assert();
        and.assert();
        (!&not).assert();

        let one2 = one.repeat();
        let two2 = two.repeat();
        let or2 = or.repeat();
        let and2 = and.repeat();
        let not2 = not.repeat();

        (!&one2).assert();
        (!&two2).assert();
        (!&or2).assert();
        (!&and2).assert();
        (!(!&not2)).assert();

        info!("one");
        info!("two");

        one2.assert();
        two2.assert();
        or2.assert();
        and2.assert();
        (!&not2).assert();

        drop(guard);
    }

    #[test]
    fn reset() {
        let asserter = Layer::default();
        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(asserter.clone());
        let guard = tracing::subscriber::set_default(subscriber);

        let one = asserter.matches("one");
        let two = asserter.matches("two");
        let or = &one | &two;
        let and = &one & &two;
        let not = !&one;

        not.assert().reset();

        info!("one");
        info!("two");

        one.assert().reset();
        two.assert().reset();
        or.assert().reset();
        and.assert().reset();

        (!&one).assert();
        (!&two).assert();
        (!&or).assert();
        (!&and).assert();
        (!&not).assert();

        info!("one");
        info!("two");

        one.assert();
        two.assert();
        or.assert();
        and.assert();
        (!&not).assert();

        drop(guard);
    }
}
