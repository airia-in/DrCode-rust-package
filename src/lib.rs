
use sentry::ClientInitGuard;
use std::panic;
use tokio::task;

/// Configuration for the Nadeem Rust error reporting.
pub struct Config {
    pub public_key: String,
    pub project_id: String,
}

/// Initialize the Sentry client with the provided configuration and set up automatic error reporting.
///
/// # Arguments
///
/// * `config` - The configuration containing the public key and project ID.
///
/// # Returns
///
/// A `ClientInitGuard` which, when dropped, will flush all events.
pub fn init(config: Config) -> ClientInitGuard {
    let dsn = format!(
        "https://{}@pulse.drcode.ai:443/{}",
        config.public_key, config.project_id
    );

    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            attach_stacktrace: true,
            ..Default::default()
        },
    ));

    // Set up custom panic hook for automatic reporting
    let default_panic = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload().downcast_ref::<String>();
        let message = payload.map(|s| s.as_str()).unwrap_or("Unknown panic");
        
        sentry::capture_message(message, sentry::Level::Fatal);

        default_panic(panic_info);
    }));

    guard
}

/// Report an error to Sentry manually.
///
/// # Arguments
///
/// * `error` - The error to report.
pub fn report_error<E: std::error::Error + Send + Sync + 'static>(error: E) {
    sentry::capture_error(&error);
}

/// Run an asynchronous task with automatic error reporting.
///
/// # Arguments
///
/// * `future` - The future to run.
///
/// # Returns
///
/// The result of the future.
pub async fn run_with_error_reporting<F, T, E>(future: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    task::spawn(async move {
        match future.await {
            Ok(result) => Ok(result),
            Err(e) => {
                sentry::capture_error(&e);
                Err(e)
            }
        }
    }).await.unwrap_or_else(|e| {
        sentry::capture_error(&e);
        panic!("Task panicked: {:?}", e);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let config = Config {
            public_key: "test_key".to_string(),
            project_id: "test_project".to_string(),
        };
        let _guard = init(config);
    }

    #[tokio::test]
    async fn test_run_with_error_reporting() {
        let config = Config {
            public_key: "test_key".to_string(),
            project_id: "test_project".to_string(),
        };
        let _guard = init(config);

        let result = run_with_error_reporting(async {
            Ok::<_, std::io::Error>(())
        }).await;
        assert!(result.is_ok());

        let error_result = run_with_error_reporting(async {
            Err::<(), _>(std::io::Error::new(std::io::ErrorKind::Other, "Test error"))
        }).await;
        assert!(error_result.is_err());
    }
}
