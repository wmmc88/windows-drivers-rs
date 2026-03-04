//! Unified error taxonomy.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("VM error: {0}")]
    Vm(String),
    #[error("detection error: {0}")]
    Detection(String),
    #[error("deployment error: {0}")]
    Deploy(String),
    #[error("capture error: {0}")]
    Capture(String),
    #[error("companion error: {0}")]
    Companion(String),
    #[error("prerequisite not met: {0}")]
    Prerequisite(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Map to process exit code: 1 = user error, 2 = system error.
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::Detection(_) | AppError::Prerequisite(_) => 1,
            AppError::Vm(_) | AppError::Deploy(_) | AppError::Capture(_) => 2,
            AppError::Companion(_) | AppError::Other(_) => 1,
        }
    }
}

impl From<crate::deploy::DeployError> for AppError {
    fn from(e: crate::deploy::DeployError) -> Self {
        AppError::Deploy(e.to_string())
    }
}

impl From<crate::vm::VmError> for AppError {
    fn from(e: crate::vm::VmError) -> Self {
        AppError::Vm(e.to_string())
    }
}

impl From<crate::detect::DetectionError> for AppError {
    fn from(e: crate::detect::DetectionError) -> Self {
        AppError::Detection(e.to_string())
    }
}

impl From<crate::debug::CaptureError> for AppError {
    fn from(e: crate::debug::CaptureError) -> Self {
        AppError::Capture(e.to_string())
    }
}

impl From<crate::echo::EchoError> for AppError {
    fn from(e: crate::echo::EchoError) -> Self {
        AppError::Companion(e.to_string())
    }
}
