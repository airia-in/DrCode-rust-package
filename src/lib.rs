use sentry::ClientOptions;
use thiserror::Error;
use std::error::Error as StdError;
use std::panic::{self, PanicInfo};
use std::sync::Once;
// use tokio::runtime::Handle;

#[derive(Error, Debug)]
pub enum DrCodeError {
    #[error("Missing required configuration field: {0}")]
    MissingField(String),
    #[error("Failed to initialize Sentry: {0}")]
    InitializationError(String),
    #[error("Application error: {0}")]
    ApplicationError(Box<dyn StdError + Send + Sync>),
}

pub struct Config {
    pub public_key: String,
    pub project_id: String,
    pub traces_sample_rate: Option<f32>,
}

pub struct DrCode {
    _sentry_client: sentry::ClientInitGuard,
}

pub trait ReportableError: StdError + Send + Sync + 'static {}

impl<T: StdError + Send + Sync + 'static> ReportableError for T {}

impl DrCode {
    pub fn new(config: Config) -> Result<Self, DrCodeError> {
        Self::validate_config(&config)?;
        
        let dsn = Self::construct_dsn(&config)?;

        let options = ClientOptions {
            dsn: Some(dsn),
            traces_sample_rate: config.traces_sample_rate.unwrap_or(1.0),
            ..Default::default()
        };
        
        let guard = sentry::init(options);
        Ok(Self { _sentry_client: guard })
    }

    fn validate_config(config: &Config) -> Result<(), DrCodeError> {
        if config.public_key.is_empty() {
            return Err(DrCodeError::MissingField("public_key".to_string()));
        }
        if config.project_id.is_empty() {
            return Err(DrCodeError::MissingField("project_id".to_string()));
        }
        Ok(())
    }

    fn construct_dsn(config: &Config) -> Result<sentry::types::Dsn, DrCodeError> {
        let dsn_string = format!(
            "https://{}@pulse.drcode.ai:443/{}",
            config.public_key, config.project_id
        );
        dsn_string.parse().map_err(|e: sentry::types::ParseDsnError| DrCodeError::InitializationError(e.to_string()))
    }

    pub fn capture_message(&self, message: &str, level: sentry::Level) {
        sentry::capture_message(message, level);
    }

    pub fn capture_error(&self, error: &(dyn StdError + Send + Sync + 'static)) {
        sentry::capture_error(error);
    }

    pub fn report<E: ReportableError>(&self, error: E) -> DrCodeError {
        self.capture_error(&error);
        DrCodeError::ApplicationError(Box::new(error))
    }

    pub fn install_error_handlers(&self) {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // Set up panic hook
            let old_hook = panic::take_hook();
            panic::set_hook(Box::new(move |panic_info: &PanicInfo<'_>| {
                Self::handle_panic(panic_info);
                old_hook(panic_info);
            }));

            // Set up hook for unhandled errors in async code
            tokio::runtime::Builder::new_multi_thread()
                .on_thread_stop(|| {
                    if std::thread::panicking() {
                        sentry::capture_message("Thread panicked", sentry::Level::Error);
                    }
                })
                .build()
                .unwrap();
        });
    }

    fn handle_panic(panic_info: &PanicInfo<'_>) {
        let payload = panic_info.payload().downcast_ref::<String>();
        let message = payload.map(|s| s.as_str()).unwrap_or("Unknown panic occurred");
        
        sentry::capture_event(sentry::protocol::Event {
            message: Some(message.to_string()),
            level: sentry::Level::Fatal,
            ..Default::default()
        });
    }
}

pub fn setup_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload().downcast_ref::<String>();
        let message = payload.map(|s| s.as_str()).unwrap_or("Unknown panic message");
        
        sentry::capture_event(sentry::protocol::Event {
            message: Some(message.to_string()),
            level: sentry::Level::Fatal,
            ..Default::default()
        });

        default_panic(panic_info);
    }));
}

pub use sentry::Level;

// Trait for automatically reporting errors
pub trait AutoReportError<T, E: std::error::Error> {
    fn report_error(self, drcode: &DrCode) -> Result<T, DrCodeError>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> AutoReportError<T, E> for Result<T, E> {
    fn report_error(self, drcode: &DrCode) -> Result<T, DrCodeError> {
        self.map_err(|e| {
            drcode.capture_error(&e);
            DrCodeError::ApplicationError(Box::new(e))
        })
    }
}

// Example usage:
#[derive(Error, Debug)]
pub enum MyAppError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let valid_config = Config {
            public_key: "valid_key".to_string(),
            project_id: "valid_id".to_string(),
            traces_sample_rate: Some(0.5),
        };
        assert!(DrCode::validate_config(&valid_config).is_ok());

        let invalid_config = Config {
            public_key: "".to_string(),
            project_id: "valid_id".to_string(),
            traces_sample_rate: None,
        };
        assert!(matches!(
            DrCode::validate_config(&invalid_config),
            Err(DrCodeError::MissingField(field)) if field == "public_key"
        ));
    }

    #[test]
    fn test_construct_dsn() {
        let config = Config {
            public_key: "c678586e903a4c8688b019ca06af7008".to_string(),
            project_id: "229".to_string(),
            traces_sample_rate: None,
        };
        let dsn = DrCode::construct_dsn(&config).unwrap();
        assert_eq!(dsn.scheme(), sentry::types::Scheme::Https);
        assert_eq!(dsn.public_key(), "c678586e903a4c8688b019ca06af7008");
        assert_eq!(dsn.host(), "pulse.drcode.ai");
        assert_eq!(dsn.port(), 443);
        assert_eq!(dsn.path(), "/");
    }
}

// Example of how to use the package in a main function
fn main() -> Result<(), DrCodeError> {
    let config = Config {
        public_key: "c678586e903a4c8688b019ca06af7008".to_string(),
        project_id: "229".to_string(),
        traces_sample_rate: Some(1.0),
    };
    
    let drcode = DrCode::new(config)?;
    drcode.install_error_handlers();

    // Your application code here
    some_function_that_might_fail().report_error(&drcode)?;

    Ok(())
}

fn some_function_that_might_fail() -> Result<(), MyAppError> {
    // Some logic that might fail
    Err(MyAppError::DatabaseError("Connection failed".to_string()))
}