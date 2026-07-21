mod tree;
use tree::*;

use fuser::{
    Config, Errno, FileAttr, FileHandle, FileType, Filesystem, Generation, INodeNo, LockOwner, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};

use std::{collections::HashMap, ffi::OsStr, sync::RwLock, time::{Duration, SystemTime, UNIX_EPOCH}};

const ROOT_DIR_INO: u64 = 1;
const SUB_DIR_INO: u64 = 4;
const HELLO_FILE_INO: u64 = 2;
const WORLD_FILE_INO: u64 = 3;
const LAZY_DIR_INO: u64 = 5;
const LAZY_FILE_INO: u64 = 6;

const HELLO_FILE_NAME: &str = "hello.txt";
const WORLD_FILE_NAME: &str = "world.txt";
const SUB_DIR_NAME: &str = "subdir";
const HELLO_CONTENT: &str = "Hello, FUSE with Rust!\n";
const WORLD_CONTENT: &str = "World, FUSE with Rust!\n";

const TTL: Duration = Duration::from_secs(1);

const NOW:SystemTime = UNIX_EPOCH;

struct MemoryFileSystem{
    stete_in_mem: RwLock<StateInMem>,
}

impl MemoryFileSystem {
    fn get_attribute(&self, ino: INodeNo) -> Option<FileAttr> {
        match self.stete_in_mem.read() {
            Ok(v) => {
                if let Some(stateinmem) = v.get_file_obj(ino) {
                    match stateinmem {
                        StateInMem::Dir { dir_name: _, file_attr, childs: _ } => {
                            Some(file_attr.clone())
                        }
                        StateInMem::File { file_name: _, file_attr, contents: _ } => {
                            Some(file_attr.clone())
                        }
                        StateInMem::LazyDir { dir_name: _, file_attr } => {
                            Some(file_attr.clone())
                        }
                        StateInMem::LazyFile { file_name: _, file_attr } => {
                            Some(file_attr.clone())
                        }
                    }
                } else {
                    None
                }
            }
            Err(e) => {
                println!("{:?}", e);
                None
            }
        }
    }
}

impl Filesystem for MemoryFileSystem {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        println!("[FUSE] lookup called: parent={}, name={:?}", parent, name);

        match self.stete_in_mem.read() {
            Ok(v) => {
                if let Some(attr) = v.get_file_obj_with_parent(&INodeNo::ROOT, &parent, &name.to_string_lossy()) {
                    reply.entry(&TTL, attr, Generation(0));
                    return;
                }
            }
            Err(e) => {
                println!("{:?}", e);
            }
        }
        reply.error(Errno::ENOENT);
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<fuser::FileHandle>, reply: ReplyAttr) {
         println!("[FUSE] getattr called: ino={}", ino);

         if let Some(attr) = self.get_attribute(ino) {
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

    let tree = StateInMem::Dir {

        dir_name: "root".to_string(),
        file_attr: FileAttr {
            ino: INodeNo::ROOT,
            size: 0,
            blocks: 0,
            atime: NOW,
            mtime: NOW,
            ctime: NOW,
            crtime: NOW,
            kind: FileType::Directory,
            perm: 0o755, // rwxr-xr-x
            nlink: 3,    // "." ".." "subdir"
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 512,
            flags: 0,
        },

        childs: vec![
            (INodeNo(HELLO_FILE_INO), StateInMem::LazyFile { 
                file_name: "hello.txt".to_string(), 
                file_attr: FileAttr {
                    ino: INodeNo(HELLO_FILE_INO),
                    size: HELLO_CONTENT.len() as u64,
                    blocks: 1,
                    atime: NOW,
                    mtime: NOW,
                    ctime: NOW,
                    crtime: NOW,
                    kind: FileType::RegularFile,
                    perm: 0o644, // rw-r--r--
                    nlink: 1,
                    uid: 1000,
                    gid: 1000,
                    rdev: 0,
                    blksize: 512,
                    flags: 0,
                }
            }),

            (
            INodeNo(SUB_DIR_INO),
            StateInMem::Dir {
                dir_name: "subdir".to_string(), 
                file_attr:  FileAttr {
                    ino: INodeNo(SUB_DIR_INO),
                    size: 0,
                    blocks: 0,
                    atime: NOW,
                    mtime: NOW,
                    ctime: NOW,
                    crtime: NOW,
                    kind: FileType::Directory,
                    perm: 0o755, // rwxr-xr-x
                    nlink: 2,    // "." ".."
                    uid: 1000,
                    gid: 1000,
                    rdev: 0,
                    blksize: 512,
                    flags: 0,
                },

                childs: vec![
                    (
                    INodeNo(LAZY_FILE_INO),
                    StateInMem::LazyFile {
                        file_name: "lazy.txt".to_string(),
                        file_attr:  FileAttr {
                            ino: INodeNo(LAZY_FILE_INO),
                            size: WORLD_CONTENT.len() as u64,
                            blocks: 1,
                            atime: NOW,
                            mtime: NOW,
                            ctime: NOW,
                            crtime: NOW,
                            kind: FileType::RegularFile,
                            perm: 0o644, // rw-r--r--
                            nlink: 1,
                            uid: 1000,
                            gid: 1000,
                            rdev: 0,
                            blksize: 512,
                            flags: 0,
                        }
                    },
                    )
                ].iter().fold(HashMap::new(), |mut acc, (k, v)| {
                    acc.insert(k.clone(), v.clone()); 
                    acc
                })
            }),

            (
            INodeNo(LAZY_DIR_INO),
            StateInMem::LazyDir { 
                dir_name: "lazydir".to_string(),
                file_attr: FileAttr {
                    ino: INodeNo(LAZY_DIR_INO),
                    size: 0,
                    blocks: 0,
                    atime: NOW,
                    mtime: NOW,
                    ctime: NOW,
                    crtime: NOW,
                    kind: FileType::Directory,
                    perm: 0o755, // rwxr-xr-x
                    nlink: 2,    // "." ".."
                    uid: 1000,
                    gid: 1000,
                    rdev: 0,
                    blksize: 512,
                    flags: 0,
                }
            }
            )
        ].iter().fold(HashMap::new(), |mut acc, (k, v)| {acc.insert(k.clone(), v.clone()); acc})};


    println!("Mounting FileSystem at {}...", mountpoint);
    fuser::mount2(MemoryFileSystem{ stete_in_mem: RwLock::new(tree) }, mountpoint, &config).unwrap();
}
