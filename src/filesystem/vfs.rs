//! Shared types and interface for filesystem implementations.

use alloc::string::String;
use alloc::vec::Vec;

/// The kind of an entry returned when listing a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
}

/// An entry in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub kind: NodeKind,
}

/// Errors that can occur while accessing a filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    InvalidPath,
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
}

/// Operations that a filesystem implementation makes available to the kernel.
pub trait FileSystem {
    fn create_dir(&mut self, path: &str) -> Result<(), FsError>;
    fn create_file(&mut self, path: &str) -> Result<(), FsError>;
    fn write_file(&mut self, path: &str, contents: &[u8]) -> Result<(), FsError>;
    fn read_file(&self, path: &str) -> Result<&[u8], FsError>;
    fn list_dir(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError>;
}

/// Splits an absolute path into components.
///
/// Repeated separators and dot components are ignored. Parent traversal is
/// deliberately rejected until the VFS has a current-working-directory model.
pub(crate) fn parse_path(path: &str) -> Result<Vec<&str>, FsError> {
    if !path.starts_with('/') || path.as_bytes().contains(&0) {
        return Err(FsError::InvalidPath);
    }

    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(FsError::InvalidPath),
            name => components.push(name),
        }
    }

    Ok(components)
}
