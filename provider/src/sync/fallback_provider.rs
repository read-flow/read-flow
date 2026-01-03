//! A provider that tries multiple sources in order until one succeeds.
//!
//! The `FallbackProvider` wraps two providers and tries the primary first,
//! falling back to the secondary if the primary fails. This is useful for
//! operations that might fail on one source but succeed on another, such as
//! file operations that prefer local sources but can fall back to remote sources.
//!
//! # Example
//!
//! ```ignore
//! use provider::r#async::{FallbackProvider, Provider};
//!
//! // Create providers for different sources
//! let local_provider = LocalFileProvider::new(path);
//! let remote_provider = RemoteFileProvider::new(url);
//!
//! // Create a fallback provider that tries local first, then remote
//! let fallback = FallbackProvider::new(local_provider, remote_provider);
//!
//! // This will try local first, and if it fails, try remote
//! let result = fallback.provide()?;
//! ```
//!
//! For longer chains, use the `or_fallback` combinator:
//!
//! ```ignore
//! let provider = first.or_fallback(second).or_fallback(third);
//! ```

use super::Provider;

/// A provider that tries a primary provider first, then falls back to a secondary.
///
/// If the primary provider succeeds, its result is returned.
/// If the primary provider fails, the fallback provider is tried.
/// If both fail, the error from the fallback provider is returned.
pub struct FallbackProvider<P1, P2> {
    primary: P1,
    fallback: P2,
}

impl<P1, P2> FallbackProvider<P1, P2> {
    /// Create a new `FallbackProvider` with a primary and fallback provider.
    ///
    /// The primary provider is tried first. If it fails, the fallback is tried.
    pub fn new(primary: P1, fallback: P2) -> Self {
        Self { primary, fallback }
    }

    /// Alias for `new` - creates a fallback provider with primary and fallback.
    pub fn with_fallback(primary: P1, fallback: P2) -> Self {
        Self::new(primary, fallback)
    }
}

impl<T, E, P1, P2> Provider<T> for FallbackProvider<P1, P2>
where
    P1: Provider<T, Error = E> + Sync,
    P2: Provider<T, Error = E> + Sync,
    T: Send,
    E: Send,
{
    type Error = E;

    fn provide(&self) -> Result<T, Self::Error> {
        match self.primary.provide() {
            Ok(value) => Ok(value),
            Err(_) => self.fallback.provide(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use super::*;

    struct FailingProvider {
        call_count: AtomicUsize,
    }

    impl FailingProvider {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    impl Provider<String> for FailingProvider {
        type Error = &'static str;

        fn provide(&self) -> Result<String, Self::Error> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Err("always fails")
        }
    }

    struct SuccessProvider {
        value: String,
        call_count: AtomicUsize,
    }

    impl SuccessProvider {
        fn new(value: &str) -> Self {
            Self {
                value: value.to_string(),
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    impl Provider<String> for SuccessProvider {
        type Error = &'static str;

        fn provide(&self) -> Result<String, Self::Error> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(self.value.clone())
        }
    }

    #[test]
    fn test_fallback_uses_first_success() {
        let primary = std::sync::Arc::new(SuccessProvider::new("primary"));
        let fallback = std::sync::Arc::new(SuccessProvider::new("fallback"));

        let provider = FallbackProvider::with_fallback(primary.clone(), fallback.clone());

        let result = provider.provide().unwrap();
        assert_eq!(result, "primary");
        assert_eq!(primary.calls(), 1);
        assert_eq!(fallback.calls(), 0); // Fallback should not be called
    }

    #[test]
    fn test_fallback_tries_next_on_failure() {
        let primary = std::sync::Arc::new(FailingProvider::new());
        let fallback = std::sync::Arc::new(SuccessProvider::new("fallback"));

        let provider = FallbackProvider::with_fallback(primary.clone(), fallback.clone());

        let result = provider.provide().unwrap();
        assert_eq!(result, "fallback");
        assert_eq!(primary.calls(), 1);
        assert_eq!(fallback.calls(), 1);
    }

    #[test]
    fn test_fallback_returns_last_error_when_all_fail() {
        let primary = std::sync::Arc::new(FailingProvider::new());
        let fallback = std::sync::Arc::new(FailingProvider::new());

        let provider = FallbackProvider::with_fallback(primary.clone(), fallback.clone());

        let result = provider.provide();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "always fails");
        assert_eq!(primary.calls(), 1);
        assert_eq!(fallback.calls(), 1);
    }

    #[test]
    fn test_chained_fallbacks() {
        let p1 = std::sync::Arc::new(FailingProvider::new());
        let p2 = std::sync::Arc::new(FailingProvider::new());
        let p3 = std::sync::Arc::new(SuccessProvider::new("third"));

        // Chain fallbacks using or_fallback combinator
        let provider = p1.clone().or_fallback(p2.clone()).or_fallback(p3.clone());

        let result = provider.provide().unwrap();
        assert_eq!(result, "third");
        assert_eq!(p1.calls(), 1);
        assert_eq!(p2.calls(), 1);
        assert_eq!(p3.calls(), 1);
    }
}
