
#[macro_export]
macro_rules! rapid_debug {
    ($($arg:tt)*) => {
        if log::log_enabled!(log::Level::Debug) {
            log::debug!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! rapid_info {
    ($($arg:tt)*) => {
        if log::log_enabled!(log::Level::Info) {
            log::info!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! rapid_warn {
    ($($arg:tt)*) => {
        if log::log_enabled!(log::Level::Warn) {
            log::warn!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! rapid_error {
    ($($arg:tt)*) => {
        if log::log_enabled!(log::Level::Error) {
            log::error!($($arg)*);
        }
    };
}
