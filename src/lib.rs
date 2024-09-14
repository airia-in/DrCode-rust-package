use sentry::{ClientOptions};
use thiserror::Error;

// Custom error enum for handling configuration and initialization errors.
#[derive(Error, Debug)]
pub enum DrCodeError {
    #[error("Missing required configuration field: {0}")]
    MissingField(String),
    #[error("Failed to initialize Sentry: {0}")]
    InitializationError(String),
}

// Configuration struct that holds Sentry-related configuration.
pub struct Config {
    pub public_key: String,
    pub project_id: String,
    pub traces_sample_rate: Option<f32>,
}

// Main struct for DrCode, holding the Sentry client guard.
pub struct DrCode {
    _sentry_client: sentry::ClientInitGuard,
}

impl DrCode {
    // Initialize DrCode with the given configuration.
    pub fn new(config: Config) -> Result<Self, DrCodeError> {
        // Validate the configuration fields
        Self::validate_config(&config)?;
        
        // Construct the DSN URL for Sentry
        let dsn = Self::construct_dsn(&config)?;

        // Initialize Sentry with the given options
        let options = ClientOptions {
            dsn: Some(dsn),
            traces_sample_rate: config.traces_sample_rate.unwrap_or(1.0),
            ..Default::default()
        };
        
        let guard = sentry::init(options);
        Ok(Self { _sentry_client: guard })
    }

    // Validates the configuration to ensure required fields are provided.
    fn validate_config(config: &Config) -> Result<(), DrCodeError> {
        if config.public_key.is_empty() {
            return Err(DrCodeError::MissingField("public_key".to_string()));
        }
        if config.project_id.is_empty() {
            return Err(DrCodeError::MissingField("project_id".to_string()));
        }
        Ok(())
    }

    // Constructs the DSN (Data Source Name) string for Sentry from the configuration.
    fn construct_dsn(config: &Config) -> Result<sentry::types::Dsn, DrCodeError> {
        let dsn_string = format!(
            "https://{}@pulse.drcode.ai:443/{}",
            config.public_key, config.project_id
        );
        dsn_string.parse().map_err(|e: sentry::types::ParseDsnError| DrCodeError::InitializationError(e.to_string()))
    }

    // Captures a message with a given severity level (Info, Warning, Error, etc.)
    pub fn capture_message(&self, message: &str, level: Level) {
        sentry::capture_message(message, level);
    }

    // Captures an error that implements the `Error`, `Send`, and `Sync` traits.
    pub fn capture_error<E>(&self, error: &E)
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        sentry::capture_error(error);
    }
}

// Set up a panic hook to capture any panics and send them to Sentry as fatal events.
pub fn setup_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload().downcast_ref::<String>();
        let message = payload.map(|s| s.as_str()).unwrap_or("Unknown panic message");
        
        // Capture the panic message and send it as a fatal error to Sentry.
        sentry::capture_event(sentry::protocol::Event {
            message: Some(message.to_string()),
            level: Level::Fatal,
            ..Default::default()
        });

        // Call the default panic hook after capturing the event.
        default_panic(panic_info);
    }));
}

// Re-export `sentry::Level` so that users of this package can access `drcode::Level`.
pub use sentry::Level;
