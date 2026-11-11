use std::{
    fs::File,
    io::{self, Read},
    sync::atomic::AtomicBool,
};

use zip::ZipArchive;

use crate::{Limits, LoadError, check_cancelled};

pub(crate) struct Package<'a> {
    zip: ZipArchive<File>,
    limits: &'a Limits,
    cancelled: &'a AtomicBool,
    expanded: u64,
}

impl<'a> Package<'a> {
    pub(crate) fn new(
        file: File,
        limits: &'a Limits,
        cancelled: &'a AtomicBool,
    ) -> Result<Self, LoadError> {
        let mut zip = ZipArchive::new(file)?;
        if zip.len() > limits.entries {
            return Err(LoadError::limit("ZIP entry count", limits.entries));
        }
        let mut declared = 0u64;
        for index in 0..zip.len() {
            check_cancelled(cancelled)?;
            let entry = zip.by_index_raw(index)?;
            let name = std::str::from_utf8(entry.name_raw())
                .map_err(|_| LoadError::Path("non-UTF-8 ZIP entry name".into()))?;
            validate_path(name.strip_suffix('/').unwrap_or(name))?;
            declared = declared.saturating_add(entry.size());
            if declared > limits.expanded_bytes {
                return Err(LoadError::limit(
                    "expanded archive size",
                    limits.expanded_bytes,
                ));
            }
        }
        Ok(Self {
            zip,
            limits,
            cancelled,
            expanded: 0,
        })
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.zip.index_for_name(name).is_some()
    }

    pub(crate) fn read(&mut self, name: &str) -> Result<Vec<u8>, LoadError> {
        if !self.contains(name) {
            return Err(LoadError::Missing(name.into()));
        }
        let entry = self.zip.by_name(name)?;
        let remaining = self.limits.expanded_bytes - self.expanded;
        let (resource, maximum, budget) = if remaining < self.limits.part_bytes {
            (
                "expanded archive size",
                self.limits.expanded_bytes,
                remaining,
            )
        } else {
            (
                "expanded part size",
                self.limits.part_bytes,
                self.limits.part_bytes,
            )
        };
        if entry.size() > budget {
            return Err(LoadError::limit(resource, maximum));
        }
        let mut data = Vec::new();
        let mut entry = entry.take(budget.saturating_add(1));
        let mut buffer = [0u8; 32 * 1024];
        loop {
            check_cancelled(self.cancelled)?;
            match entry.read(&mut buffer) {
                Ok(0) => break,
                Ok(len) => data.extend_from_slice(&buffer[..len]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(source) => {
                    return Err(LoadError::Read {
                        part: name.into(),
                        source,
                    });
                }
            }
        }
        if data.len() as u64 > budget {
            return Err(LoadError::limit(resource, maximum));
        }
        self.expanded += data.len() as u64;
        Ok(data)
    }
}

fn validate_path(path: &str) -> Result<(), LoadError> {
    if path.contains(['\\', ':', '%', '?', '#'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(LoadError::Path(path.into()));
    }
    Ok(())
}

pub(crate) fn resolve_target(owner: &str, target: &str) -> Result<String, LoadError> {
    if let Some(absolute) = target.strip_prefix('/') {
        validate_path(absolute)?;
        return Ok(absolute.into());
    }
    validate_path(target)?;
    Ok(match owner.rsplit_once('/') {
        Some((folder, _)) => format!("{folder}/{target}"),
        None => target.into(),
    })
}
