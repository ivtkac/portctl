use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Authentication failed for {service} at {host}")]
    AuthFailed { service: String, host: String },

    #[error("No JWT token received from {service}")]
    NoJwtToken { service: String },

    #[error("Stack template '{name}' not found (checked: {path})")]
    TemplateNotFound { name: String, path: String },

    #[error("API error [{status}] during '{operation}': {body}")]
    ApiError {
        status: u16,
        operation: String,
        body: String,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Endpoint '{name}' not found on Portainer")]
    EndpointNotFound { name: String },

    #[error("{0}")]
    Other(String),
}
