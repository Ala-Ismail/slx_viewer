//! Headless, read-only loader for saved Simulink diagrams.

mod archive;
pub mod model;
mod parser;
mod xml;

use std::{
    fs::File,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use thiserror::Error;

use model::Model;

#[derive(Clone, Debug)]
pub struct Limits {
    pub archive_bytes: u64,
    pub entries: usize,
    pub expanded_bytes: u64,
    pub part_bytes: u64,
    pub xml_depth: usize,
    pub xml_elements: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            archive_bytes: 128 * 1024 * 1024,
            entries: 4096,
            expanded_bytes: 128 * 1024 * 1024,
            part_bytes: 16 * 1024 * 1024,
            xml_depth: 96,
            xml_elements: 500_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("loading cancelled")]
    Cancelled,
    #[error("cannot open {path}: {source}")]
    Open {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid or unsupported ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("cannot read package part {part}: {source}")]
    Read {
        part: String,
        source: std::io::Error,
    },
    #[error("invalid package path: {0}")]
    Path(String),
    #[error("missing package part: {0}")]
    Missing(String),
    #[error("{resource} limit exceeded (maximum {maximum})")]
    Limit {
        resource: &'static str,
        maximum: u64,
    },
    #[error("{part}: {message}")]
    Format { part: String, message: String },
}

impl LoadError {
    fn format(part: &str, message: impl Into<String>) -> Self {
        Self::Format {
            part: part.into(),
            message: message.into(),
        }
    }

    fn limit(resource: &'static str, maximum: impl TryInto<u64>) -> Self {
        Self::Limit {
            resource,
            maximum: maximum.try_into().unwrap_or(u64::MAX),
        }
    }
}

pub fn load(path: impl AsRef<Path>, limits: &Limits) -> Result<Model, LoadError> {
    load_cancellable(path, limits, &AtomicBool::new(false))
}

/// Stops at the next cancellation checkpoint when the flag is set.
pub fn load_cancellable(
    path: impl AsRef<Path>,
    limits: &Limits,
    cancelled: &AtomicBool,
) -> Result<Model, LoadError> {
    check_cancelled(cancelled)?;
    let path = path.as_ref();
    let open_error = |source| LoadError::Open {
        path: path.display().to_string(),
        source,
    };
    let file = File::open(path).map_err(open_error)?;
    if file.metadata().map_err(open_error)?.len() > limits.archive_bytes {
        return Err(LoadError::limit(
            "compressed archive size",
            limits.archive_bytes,
        ));
    }
    let name = path
        .file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    let model = parser::parse(
        archive::Package::new(file, limits, cancelled)?,
        &name,
        limits,
        cancelled,
    )?;
    check_cancelled(cancelled)?;
    Ok(model)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), LoadError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(LoadError::Cancelled);
    }
    Ok(())
}
