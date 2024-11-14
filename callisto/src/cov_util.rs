#[macro_export]
macro_rules! debug {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::debug!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::debug!($($arg)+);
        }
     };
}

#[macro_export]
macro_rules! info {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::info!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::info!($($arg)+);
        }
     };
}

#[macro_export]
macro_rules! warn {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::warn!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::warn!($($arg)+);
        }
     };
}

#[macro_export]
macro_rules! error {
    (target: $target:expr, $($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::error!(target: $target, $($arg)+);
        }
    };
    ($($arg:tt)+) => {
        #[cfg(not(coverage))]
        {
            log::error!($($arg)+);
        }
     };
}
