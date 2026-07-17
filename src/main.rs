use fuser::{
    Config, Errno, FileAttr, FileHandle, FileType, Filesystem, Generation, INodeNo, LockOwner, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};

use std::{ffi::OsStr, time::{Duration, UNIX_EPOCH}};

const ROOT_DIR_INO: u64 = 1;
const SUB_DIR_INO: u64 = 4;
const HELLO_FILE_INO: u64 = 2;
const WORLD_FILE_INO: u64 = 3;
const HELLO_FILE_NAME: &str = "hello.txt";
const WORLD_FILE_NAME: &str = "world.txt";
const SUB_DIR_NAME: &str = "subdir";
const HELLO_CONTENT: &str = "Hello, FUSE with Rust!\n";
const WORLD_CONTENT: &str = "World, FUSE with Rust!\n";

const TTL: Duration = Duration::from_secs(1);

struct MemoryFileSystem;

impl MemoryFileSystem {
    fn get_attribute(&self, ino: u64) -> Option<FileAttr> {
        let now = UNIX_EPOCH;

        match ino {
            ROOT_DIR_INO => Some(FileAttr {
                ino: INodeNo::ROOT,
                size: 0,
                blocks: 0,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: FileType::Directory,
                perm: 0o755, // rwxr-xr-x
                nlink: 3,    // "." ".." "subdir"
                uid: 1000,
                gid: 1000,
                rdev: 0,
                blksize: 512,
                flags: 0,
            }),
            SUB_DIR_INO => Some(FileAttr {
                ino: INodeNo(SUB_DIR_INO),
                size: 0,
                blocks: 0,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: FileType::Directory,
                perm: 0o755, // rwxr-xr-x
                nlink: 2,    // "." ".."
                uid: 1000,
                gid: 1000,
                rdev: 0,
                blksize: 512,
                flags: 0,
            }),
            HELLO_FILE_INO => Some(FileAttr {
                ino: INodeNo(HELLO_FILE_INO),
                size: HELLO_CONTENT.len() as u64,
                blocks: 1,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: FileType::RegularFile,
                perm: 0o644, // rw-r--r--
                nlink: 1,
                uid: 1000,
                gid: 1000,
                rdev: 0,
                blksize: 512,
                flags: 0,
            }),
            WORLD_FILE_INO => Some(FileAttr {
                ino: INodeNo(WORLD_FILE_INO),
                size: WORLD_CONTENT.len() as u64,
                blocks: 1,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: FileType::RegularFile,
                perm: 0o644, // rw-r--r--
                nlink: 1,
                uid: 1000,
                gid: 1000,
                rdev: 0,
                blksize: 512,
                flags: 0,
            }),
            _ => None,
        }
    }
}

impl Filesystem for MemoryFileSystem {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        println!("[FUSE] lookup called: parent={}, name={:?}", parent, name);

        if parent == INodeNo::ROOT {
            if name.to_str() == Some(HELLO_FILE_NAME) {
                if let Some(attr) = self.get_attribute(HELLO_FILE_INO) {
                    reply.entry(&TTL, &attr, Generation(0));
                    return;
                }
            } else if name.to_str() == Some(SUB_DIR_NAME) {
                if let Some(attr) = self.get_attribute(SUB_DIR_INO) {
                    reply.entry(&TTL, &attr, Generation(0));
                    return;
                }
            } else {
                println!("name.to_str() = {:?}", name.to_str());
            }
        } else if parent == INodeNo(SUB_DIR_INO) {
            if name.to_str() == Some(WORLD_FILE_NAME) {
                if let Some(attr) = self.get_attribute(WORLD_FILE_INO) {
                    reply.entry(&TTL, &attr, Generation(0));
                    return;
                }
            } else {
                println!("name.to_str() = {:?}", name.to_str());
            }
        }
        reply.error(Errno::ENOENT);
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<fuser::FileHandle>, reply: ReplyAttr) {
         println!("[FUSE] getattr called: ino={}", ino);

         if let Some(attr) = self.get_attribute(ino.0) {
             reply.attr(&TTL, &attr);
         } else {
             reply.error(Errno::ENOENT);
         }
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        println!("[FUSE] readdir called: ino={}, offset={}", ino, offset);

        if ino == INodeNo::ROOT {
            if offset == 0 { // TODO
                let _ = reply.add(INodeNo::ROOT, 1, FileType::Directory, ".")
                    || reply.add(INodeNo::ROOT, 2, FileType::Directory, "..")
                    || reply.add(INodeNo(HELLO_FILE_INO), 3, FileType::RegularFile, HELLO_FILE_NAME)
                    || reply.add(INodeNo(SUB_DIR_INO), 4, FileType::Directory, SUB_DIR_NAME);
            }
            reply.ok();
        } else if ino == INodeNo(SUB_DIR_INO) {
            if offset == 0 {
                let _ = reply.add(INodeNo(SUB_DIR_INO), 1, FileType::Directory, ".")
                    || reply.add(INodeNo::ROOT, 2, FileType::Directory, "..")
                    || reply.add(INodeNo(WORLD_FILE_INO), 3, FileType::RegularFile, WORLD_FILE_NAME);
            }
            reply.ok();
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        println!(
            "[FUSE] read called: ino={}, offset={}, size={}",
            ino, offset, size
        );

        if ino == INodeNo(HELLO_FILE_INO) {
            let bytes = HELLO_CONTENT.as_bytes();
            let start = offset as usize;
            if start >= bytes.len() {
                reply.data(&[]);
                return;
            }
            let end = std::cmp::min(start + size as usize, bytes.len());
            reply.data(&bytes[start..end]);
        } else if ino == INodeNo(WORLD_FILE_INO) {
            let bytes = WORLD_CONTENT.as_bytes();
            let start = offset as usize;
            if start >= bytes.len() {
                reply.data(&[]);
                return;
            }
            let end = std::cmp::min(start + size as usize, bytes.len());
            reply.data(&bytes[start..end]);
        } else {
            reply.error(Errno::ENOENT);
        }
    }
}

fn main() {
    let mountpoint = "/tmp/fuse_test";
    std::fs::create_dir_all(mountpoint).unwrap();

    let options = vec![
        MountOption::RO,          // Read-Only
        MountOption::FSName("rust_fuse".to_string()),
    ];
    let mut config = Config::default();
    config.mount_options = options;
    config.n_threads = Some(4);
    config.clone_fd = true;

    println!("Mounting FileSystem at {}...", mountpoint);
    fuser::mount2(MemoryFileSystem, mountpoint, &config).unwrap();
}
