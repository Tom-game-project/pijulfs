mod tree;
use tree::*;

use fuser::{
    Config, Errno, FileAttr, FileHandle, FileType, Filesystem, Generation, INodeNo, LockOwner, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};

use std::{collections::BTreeMap, ffi::OsStr, sync::{Mutex, RwLock}, time::{Duration, SystemTime, UNIX_EPOCH}};

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

struct TestResolver {
    max_inode: u64,
}

impl LazyResolver for TestResolver {
    type Tree = StateInMem;
    type Inode = INodeNo;

    fn gen_new_inode (&mut self) -> Self::Inode {
        self.max_inode += 1;
        INodeNo(self.max_inode)
    }

    fn gen_children(&mut self, parent_ino: &Self::Inode) -> BTreeMap<Self::Inode, Self::Tree> {
        let mut r_hash_map = BTreeMap::new();
        println!("gen_children: parent inode: {}", parent_ino);
        let new_inode = self.gen_new_inode();
        r_hash_map.insert(
            new_inode,
            StateInMem::LazyFile {
                file_name: "lazy.txt".to_string(),
                file_attr:  FileAttr {
                    ino: new_inode,
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
            });

        let new_inode = self.gen_new_inode();
        r_hash_map.insert(
            new_inode,
            StateInMem::LazyDir { 
                dir_name: "lazydir".to_string(), 
                file_attr: FileAttr {
                    ino: new_inode,
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
                }
            });
        r_hash_map
    }
}

struct MemoryFileSystem{
    resolver: Mutex<TestResolver>,
    stete_in_mem: RwLock<StateInMem>,
}

impl MemoryFileSystem {
    fn get_attribute(&self, ino: INodeNo) -> Option<FileAttr> {
        match self.stete_in_mem.read() {
            Ok(v) => {
                if let Some(stateinmem) = v.get_file_obj(ino) {
                    stateinmem.get_fileattr().map(|a| a.clone())
                } else {
                    println!("Errrrrrrrr");
                    None
                }
            }
            Err(e) => {
                println!("{:?}", e);
                None
            }
        }
    }

    fn get_dir<T, F>(&self, ino: &INodeNo, resolver: &mut T, mut f: F) -> Result<(), Errno>
        where T: LazyResolver<Tree = StateInMem, Inode = INodeNo>,
              F: FnMut(&INodeNo, std::collections::btree_map::Iter<'_, INodeNo, StateInMem>) -> Result<(), Errno>
    {
        match self.stete_in_mem.write() {
            Ok(mut v) => {
                if let Some (parent_ino) = v.get_parent_inode_of(ino) {
                    if let Some(it) = v.get_crrent_dir(ino, resolver) {
                        f(&parent_ino, it)
                    } else {
                        println!("no such dir");
                        Err(Errno::ENOENT)
                    }
                } else {
                    Err(Errno::EIO)
                }
            }
            Err(e) => {
                println!("{:?}", e);
                Err(Errno::EIO)
            }
        }
    }
}

impl Filesystem for MemoryFileSystem {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        println!("[FUSE] lookup called: parent={}, name={:?}", parent, name);

        match self.resolver.lock() { // <------ lock: resolver
            Ok(mut resolver) => {
                match self.stete_in_mem.write() {
                    Ok(mut v) => {
                        if let Some(attr) = v.get_file_obj_with_parent(&parent, &name.to_string_lossy(), &mut *resolver) 
                        {
                            reply.entry(&TTL, attr, Generation(0));
                            return;
                        } else {
                            println!("Debug: not found -> parent inode {}, name {}", parent, name.to_string_lossy());
                        }
                    }
                    Err(e) => {
                        println!("{:?}", e);
                    }
                }
            } 
            Err(e) => {

            }
        }
        reply.error(Errno::ENOENT);
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<fuser::FileHandle>, reply: ReplyAttr) {
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

        match self.resolver.lock() {
            Ok(mut v) => {
                match self.get_dir(&ino, &mut *v, |parent_ino, iter| {
                    if offset == 0 {
                        if  reply.add(ino, 1, FileType::Directory, ".") {
                            return Ok(());
                        }
                    }

                    if offset <= 1 {
                        if reply.add(*parent_ino, 2, FileType::Directory, "..") {
                            return Ok(());
                        }
                    }

                    let skip_count = if offset > 2 { (offset - 2) as usize } else { 0 };

                    for (i, (child_ino, child_state)) in iter.enumerate().skip(skip_count) {
                        let next_offset = (i + 3) as u64;

                        if let Some((name, file_attr)) = child_state.get_name_and_fileattr() {
                            let kind = file_attr.kind;
                            if reply.add(*child_ino, next_offset, kind, name) {
                                break;
                            }
                        } else {
                            return Err(Errno::EIO); // TODO ERROR
                        }
                    }
                    Ok(())
                }) {
                    Ok(()) => reply.ok(),
                    Err(err) => reply.error(err),
                };
            }
            Err(_err) => {
                reply.error(Errno::EIO)
            }
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

    let tree = StateInMem::LazyDir {
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
        }};

    println!("Mounting FileSystem at {}...", mountpoint);
    fuser::mount2(MemoryFileSystem{ resolver: TestResolver { max_inode: ROOT_DIR_INO }.into() ,stete_in_mem: RwLock::new(tree) }, mountpoint, &config).unwrap();
}
