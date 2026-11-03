//! A simple tree-based filesystem stored entirely in kernel memory.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::vfs::{parse_path, DirectoryEntry, FileSystem, FsError, NodeKind};

type Directory = BTreeMap<String, Node>;

enum Node {
    Directory(Directory),
    File(Vec<u8>),
}

/// An in-memory filesystem with a single root directory.
pub struct RamFs {
    root: Directory,
}

impl RamFs {
    /// Creates an empty filesystem.
    pub fn new() -> Self {
        Self {
            root: BTreeMap::new(),
        }
    }

    fn directory_at<'a>(
        root: &'a Directory,
        components: &[&str],
    ) -> Result<&'a Directory, FsError> {
        let mut directory = root;
        for component in components {
            directory = match directory.get(*component) {
                Some(Node::Directory(child)) => child,
                Some(Node::File(_)) => return Err(FsError::NotDirectory),
                None => return Err(FsError::NotFound),
            };
        }
        Ok(directory)
    }

    fn directory_at_mut<'a>(
        root: &'a mut Directory,
        components: &[&str],
    ) -> Result<&'a mut Directory, FsError> {
        let mut directory = root;
        for component in components {
            directory = match directory.get_mut(*component) {
                Some(Node::Directory(child)) => child,
                Some(Node::File(_)) => return Err(FsError::NotDirectory),
                None => return Err(FsError::NotFound),
            };
        }
        Ok(directory)
    }

    fn parent_and_name<'a, 'b>(
        root: &'a mut Directory,
        components: &[&'b str],
    ) -> Result<(&'a mut Directory, &'b str), FsError> {
        let (name, parents) = components.split_last().ok_or(FsError::InvalidPath)?;
        let parent = Self::directory_at_mut(root, parents)?;
        // The name comes from the caller's path, not from the borrowed directory.
        Ok((parent, *name))
    }
}

impl Default for RamFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for RamFs {
    fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        let components = parse_path(path)?;
        let (parent, name) = Self::parent_and_name(&mut self.root, &components)?;
        if parent.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        parent.insert(String::from(name), Node::Directory(BTreeMap::new()));
        Ok(())
    }

    fn create_file(&mut self, path: &str) -> Result<(), FsError> {
        let components = parse_path(path)?;
        let (parent, name) = Self::parent_and_name(&mut self.root, &components)?;
        if parent.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        parent.insert(String::from(name), Node::File(Vec::new()));
        Ok(())
    }

    fn write_file(&mut self, path: &str, contents: &[u8]) -> Result<(), FsError> {
        let components = parse_path(path)?;
        let (name, parents) = components.split_last().ok_or(FsError::InvalidPath)?;
        let parent = Self::directory_at_mut(&mut self.root, parents)?;
        match parent.get_mut(*name) {
            Some(Node::File(data)) => {
                data.clear();
                data.extend_from_slice(contents);
                Ok(())
            }
            Some(Node::Directory(_)) => Err(FsError::IsDirectory),
            None => Err(FsError::NotFound),
        }
    }

    fn read_file(&self, path: &str) -> Result<&[u8], FsError> {
        let components = parse_path(path)?;
        let (name, parents) = components.split_last().ok_or(FsError::InvalidPath)?;
        let parent = Self::directory_at(&self.root, parents)?;
        match parent.get(*name) {
            Some(Node::File(data)) => Ok(data.as_slice()),
            Some(Node::Directory(_)) => Err(FsError::IsDirectory),
            None => Err(FsError::NotFound),
        }
    }

    fn list_dir(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        let components = parse_path(path)?;
        let directory = Self::directory_at(&self.root, &components)?;
        let mut entries = Vec::with_capacity(directory.len());

        for (name, node) in directory {
            let kind = match node {
                Node::Directory(_) => NodeKind::Directory,
                Node::File(_) => NodeKind::File,
            };
            entries.push(DirectoryEntry {
                name: name.clone(),
                kind,
            });
        }

        Ok(entries)
    }
}

/// Exercises the basic RAMFS operations after the kernel heap is available.
pub fn self_test() -> bool {
    let mut filesystem = RamFs::new();
    if filesystem.create_dir("/system").is_err()
        || filesystem.create_dir("/system/config").is_err()
        || filesystem.create_file("/system/config/boot.txt").is_err()
        || filesystem
            .write_file("/system/config/boot.txt", b"Sestina RAMFS")
            .is_err()
    {
        return false;
    }

    let contents_match = filesystem
        .read_file("/system/config/boot.txt")
        .map(|contents| contents == b"Sestina RAMFS")
        .unwrap_or(false);
    let listing_matches = filesystem
        .list_dir("/system/config")
        .map(|entries| {
            entries.len() == 1 && entries[0].name == "boot.txt" && entries[0].kind == NodeKind::File
        })
        .unwrap_or(false);

    contents_match && listing_matches
}
