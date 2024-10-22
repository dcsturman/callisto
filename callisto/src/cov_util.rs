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

macro_rules! warn_cov {
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

pub(crate) use debug;
pub(crate) use info;
pub(crate) use error;
pub(crate) use warn_cov;












