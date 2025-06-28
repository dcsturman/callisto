#![cfg_attr(coverage, feature(no_coverage))]

#[macro_export]
macro_rules! debug {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg_attr(coverage, no_coverage)]
        {
            tracing::debug!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::debug!($($arg)+);
        }
     };
}

#[macro_export]
macro_rules! info {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::info!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::info!($($arg)+);
        }
     };
}

#[macro_export]
macro_rules! warn {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::warn!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::warn!($($arg)+);
        }
     };
}

#[macro_export]
macro_rules! error {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::error!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            tracing::error!($($arg)+);
        }
     };
}
