use fuser::{
    FileAttr, INodeNo
};

use std::{collections::{BTreeMap, btree_map::Iter}, time::{SystemTime, UNIX_EPOCH}};

const NOW:SystemTime = UNIX_EPOCH;

#[derive(Clone, Debug)]
pub enum StateInMem {
    File {
        file_name: String,
        file_attr: FileAttr,
        contents: String,
    },
    Dir {
        dir_name: String,
        file_attr: FileAttr,
        childs: BTreeMap<INodeNo, StateInMem>
    },
    LazyDir {
        dir_name: String,
        file_attr: FileAttr
    },
    LazyFile {
        file_name: String,
        file_attr: FileAttr
    },
    Resolving
}

impl std::fmt::Display for StateInMem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn build_tree(node: &StateInMem, depth: usize, ino: Option<&INodeNo>) -> String {
            let indent = "  ".repeat(depth);
            
            let prefix = match ino {
                Some(i) => format!("- ino:{:?} ", i),
                None => String::new(),
            };

            match node {
                StateInMem::Dir { dir_name, childs, .. } => {
                    let mut s = format!("{}{}dir \"{}\"\n", indent, prefix, dir_name);
                    
                    let mut sorted_childs: Vec<_> = childs.iter().collect();
                    sorted_childs.sort_by_cached_key(|(k, _)| format!("{:?}", k));

                    for (child_ino, child) in sorted_childs {
                        s.push_str(&build_tree(child, depth + 1, Some(child_ino)));
                        s.push('\n');
                    }
                    
                    s.trim_end().to_string()
                }
                StateInMem::File { file_name, .. } => {
                    format!("{}{}file \"{}\"", indent, prefix, file_name)
                }
                StateInMem::LazyDir { dir_name, .. } => {
                    format!("{}{}lazydir \"{}\"", indent, prefix, dir_name)
                }
                StateInMem::LazyFile { file_name, .. } => {
                    format!("{}{}lazyfile \"{}\"", indent, prefix, file_name)
                }
                StateInMem::Resolving => {
                    format!("")
                }
            }
        }

        write!(f, "{}", build_tree(self, 0, None))
    }
}

pub trait LazyResolver { 
    type Tree;
    type Inode;

    fn gen_new_inode(&mut self) -> INodeNo;
    // fn gen_file_contents(&self) -> String /* TODO */;
    fn init(&mut self, target: &Self::Tree) {}

    // 遅延的にディレクトリの子要素を生成する
    fn gen_children(&mut self, target: &Self::Tree) -> BTreeMap<Self::Inode, Self::Tree>;
}

impl StateInMem {
    pub fn get_fileattr(&self) -> Option<&FileAttr> {
        match self {
            Self::Dir      { file_attr, .. } => Some(file_attr),
            Self::File     { file_attr, .. } => Some(file_attr),
            Self::LazyDir  { file_attr, .. } => Some(file_attr),
            Self::LazyFile { file_attr, .. } => Some(file_attr),
            Self::Resolving => None
        }
    }

    pub fn get_name(&self) -> Option<&String> {
        match self {
            Self::Dir      { dir_name, .. } => Some(dir_name),
            Self::File     { file_name, .. } => Some(file_name),
            Self::LazyDir  { dir_name, .. } => Some(dir_name),
            Self::LazyFile { file_name, .. } => Some(file_name),
            Self::Resolving => None,
        }
    }

    pub fn get_name_and_fileattr(&self) -> Option<(&String, &FileAttr)> {
        match self {
            Self::Dir      {  file_attr, dir_name, ..  } => Some((dir_name , file_attr)),
            Self::File     {  file_attr, file_name, .. } => Some((file_name, file_attr)),
            Self::LazyDir  {  file_attr, dir_name, ..  } => Some((dir_name , file_attr)),
            Self::LazyFile {  file_attr, file_name, .. } => Some((file_name, file_attr)),
            Self::Resolving => None,
        }
    }

    /// return resolved stateinmem tree.
    pub fn get_file_obj(&self, target_ino: INodeNo /*already generated*/) -> Option<&StateInMem> {
        match &self {
            Self::Dir { childs , .. } => {
                if let Some(a) = childs.get(&target_ino) {
                    Some(a)
                } else if target_ino == INodeNo::ROOT && self.get_fileattr().unwrap().ino == INodeNo::ROOT {
                    Some(self)
                } else {
                    childs
                        .iter()
                        .find(|(_, stateinmem)| stateinmem.get_file_obj(target_ino).is_some())
                        .map(|(_, b)| b)
                }
            }
            _ => {
                None
            }
        }
    }

    /// for lookup function
    pub fn get_file_obj_with_parent(&self, cur_ino: &INodeNo, parent_ino: &INodeNo, target_parent_ino: &INodeNo, name: &str) -> Option<&FileAttr> {
        println!("cur_ino {:?} parent_ino {:?} target_parent_ino {:?}| {}", cur_ino, parent_ino, target_parent_ino, name);
        match &self {
            Self::File { file_name, file_attr, .. } => {
                if target_parent_ino == parent_ino && file_name == name {
                    Some(file_attr)
                } else {
                    None
                }
            }
            Self::Dir { dir_name, file_attr, childs } => {
                if target_parent_ino == parent_ino && dir_name == name {
                    Some(file_attr)
                } else {
                    childs
                        .iter()
                        .find_map(|(cino, stateinmem)| stateinmem.get_file_obj_with_parent(cino, cur_ino, target_parent_ino, name))
                }
            }
            Self::LazyFile { file_name, file_attr } => {
                if target_parent_ino == parent_ino && file_name == name {
                    Some(file_attr)
                } else {
                    None
                }
            }
            Self::LazyDir { dir_name, file_attr } => {
                if target_parent_ino == parent_ino && dir_name == name {
                    Some(file_attr)
                } else {
                    None
                }
            }
            Self::Resolving => {
                None
            }
        }
    }

    pub fn get_crrent_dir<R>(&mut self, target_ino: &INodeNo, resolver: &mut R) -> Option<Iter<'_, INodeNo, StateInMem>> 
    where R: LazyResolver<Tree = Self, Inode = INodeNo>
    {
        let mut stack: Vec<(&INodeNo, &mut StateInMem)> = Vec::new();

        let ino = self
            .get_fileattr()
            .unwrap() // unreachable
            .ino;

        stack.push((&ino, self));
        while let Some((current_ino, state)) = stack.pop() {
            if matches!(state, Self::LazyDir { .. }) {
                let old = std::mem::replace(state, StateInMem::Resolving);
                let childs = resolver.gen_children(&old);

                if let Self::LazyDir { dir_name, file_attr } = old {

                    *state = StateInMem::Dir { 
                        dir_name: dir_name.to_string(), 
                        file_attr: file_attr,
                        childs
                    };
                }
            }

            if let Self::Dir { childs, .. } = state {
                if current_ino == target_ino {
                    return Some(childs.iter());
                } else {
                    for i in childs {
                        stack.push(i);
                    }
                }
            }
        }
        None
    }

    pub fn get_parent_inode_of(&self, target_ino: &INodeNo) -> Option<INodeNo> {
        if target_ino == &INodeNo::ROOT {
            Some(INodeNo::ROOT)
        } else {
            let mut stack: Vec<(&INodeNo, &INodeNo, &StateInMem)> = Vec::new();

            let ino = self
                .get_fileattr()
                .unwrap() // unreachable
                .ino;

            stack.push((&ino, &ino, self));

            while let Some((parent_ino, current_ino, state)) = stack.pop() {
                if current_ino == target_ino {
                    return Some(*parent_ino);
                }
                if let Self::Dir { childs, .. } = state {
                    for i in childs {
                        stack.push((current_ino, i.0, i.1));
                    }
                }
            }
            None
        }
    }
}


#[cfg(test)]
mod tree_test {
    use crate::tree::{LazyResolver, StateInMem};

    use fuser::{
        FileAttr, FileType, INodeNo
    };

    use std::{collections::BTreeMap, time::{Duration, SystemTime, UNIX_EPOCH}};

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

    fn set_up_test_tree() -> StateInMem {
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
                        INodeNo(WORLD_FILE_INO),
                        StateInMem::LazyFile {
                            file_name: "world.txt".to_string(),
                            file_attr:  FileAttr {
                                ino: INodeNo(WORLD_FILE_INO),
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
                    ].iter().fold(BTreeMap::new(), |mut acc, (k, v)| {
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
            ].iter().fold(BTreeMap::new(), |mut acc, (k, v)| {acc.insert(k.clone(), v.clone()); acc})};
        tree
    }

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

        fn gen_children(&mut self, tree: &Self::Tree) -> BTreeMap<Self::Inode, Self::Tree> {
            let mut r_hash_map = BTreeMap::new();
            println!("gen_children: tree: {}", tree);
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
            r_hash_map
        }
    }

    fn debug_print_dir(dir_children: &BTreeMap<INodeNo, StateInMem>) {
        for (ino, stateinmem) in dir_children {
            println!("ino{:?} name {}", ino, stateinmem);
        }
    }

    #[test]
    fn test00() {
        let mut tree = set_up_test_tree();
        let mut test_resolver = TestResolver{ max_inode: 10 };

        match &tree {
            StateInMem::Dir { dir_name, file_attr, childs } => {
                debug_print_dir(childs);
            }
            _ => {}
        }

        println!("-----------------------------------------------------------------");
        if let Some(a) = tree.get_crrent_dir(&INodeNo::ROOT, &mut test_resolver) {
            for (ino, sim) in a {
                println!("ino: {:?}, {}", ino, sim);
            }
        }
        println!("-----------------------------------------------------------------");
        if let Some(a) = tree.get_crrent_dir(&INodeNo::ROOT, &mut test_resolver) {
            for (ino, sim) in a {
                println!("ino: {:?}, {}", ino, sim);
            }
        }
        println!("-----------------------------------------------------------------");
        if let Some(a) = tree.get_crrent_dir(&INodeNo(LAZY_DIR_INO), &mut test_resolver) {
            for (ino, sim) in a {
                println!("ino: {:?}, {}", ino, sim);
            }
        }
        println!("-----------------------------------------------------------------");
        if let Some(a) = tree.get_crrent_dir(&INodeNo::ROOT, &mut test_resolver) {
            for (ino, sim) in a {
                println!("ino: {:?}, {}", ino, sim);
            }
        }
        println!("-----------------------------------------------------------------");
        match &tree {
            StateInMem::Dir { dir_name, file_attr, childs } => {
                debug_print_dir(childs);
            }
            _ => {}
        }
        println!("parent ino {}", tree.get_parent_inode_of(&INodeNo(11)).expect("parent not found"));

    }
}
