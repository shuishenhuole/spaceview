//! 目录树内存结构:Arena 中只存目录节点(u32 索引互引),
//! 文件不建节点,大小只记在直接父目录上,由扫描结束时统一聚合。

use std::path::PathBuf;

/// 单个目录节点
#[derive(Clone)]
pub struct DirNode {
    pub parent: Option<u32>,    // 父目录索引,None 表示根
    pub name: String,           // 显示名(相对父级);根节点为挂载点全路径
    pub size: u64,              // 聚合后的总字节
    pub file_count: u64,        // 聚合后的文件数
    pub children: Vec<u32>,     // 子目录索引
    pub denied: bool,           // 读取被拒(如 System Volume Information)
    /// 本次会话内已删除到回收站:大小清零但保留在父节点 children 中,
    /// Treemap/目录树以"已删除"占位块显示
    pub deleted: bool,
}

/// Arena 存储的目录树
#[derive(Clone)]
pub struct Tree {
    pub root_path: PathBuf, // 扫描根,如 C:\
    pub nodes: Vec<DirNode>,
}

/// 根节点索引
pub const ROOT: u32 = 0;

impl Tree {
    pub fn new(root_path: PathBuf) -> Self {
        let name = root_path
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_string();
        Tree {
            root_path,
            nodes: vec![DirNode {
                parent: None,
                name: name,
                size: 0,
                file_count: 0,
                children: Vec::new(),
                denied: false,
                deleted: false,
            }],
        }
    }

    pub fn push_node(&mut self, node: DirNode) -> u32 {
        self.nodes.push(node);
        (self.nodes.len() - 1) as u32
    }

    pub fn dir_path(&self, idx: u32) -> PathBuf {
        let mut names = Vec::new();
        let mut cur = idx;
        while let Some(node) = self.nodes.get(cur as usize) {
            if let Some(p) = node.parent {
                names.push(node.name.clone());
                cur = p;
            } else {
                break;
            }
        }
        names.reverse();
        let mut path = self.root_path.clone();
        for n in names {
            path.push(n);
        }
        path
    }

    pub fn subtract(&mut self, dir_idx: u32, size: u64, file_count: u64) {
        let mut cur = dir_idx;
        loop {
            let node = &mut self.nodes[cur as usize];
            node.size = node.size.saturating_sub(size);
            node.file_count = node.file_count.saturating_sub(file_count);
            match node.parent {
                Some(p) => cur = p,
                None => break,
            }
        }
    }

    /// 目录被删除后同步树:整个子树大小清零并标记"已删除",
    /// 节点保留在父 children 中作占位,沿祖先链回减
    pub fn remove_subtree(&mut self, idx: u32) {
        let (size, files) = {
            let n = &self.nodes[idx as usize];
            (n.size, n.file_count)
        };
        // 子树全部清零(倒序索引保证父先于子访问也无所谓,这里用栈遍历)
        let mut stack = vec![idx];
        while let Some(i) = stack.pop() {
            let node = &mut self.nodes[i as usize];
            node.size = 0;
            node.file_count = 0;
            stack.extend(node.children.iter().copied());
        }
        self.nodes[idx as usize].deleted = true;
        // 向祖先回减(不从父 children 摘除,保留占位)
        if let Some(p) = self.nodes[idx as usize].parent {
            self.subtract(p, size, files);
        }
    }

    /// 直接子目录按大小降序 (idx, name, size)
    // UI 已改为直接遍历 children(需读取 deleted 标记),此助手仅冒烟测试使用
    #[allow(dead_code)]
    pub fn sorted_children(&self, idx: u32) -> Vec<(u32, String, u64)> {
        let mut v: Vec<(u32, String, u64)> = self.nodes[idx as usize]
            .children
            .iter()
            .map(|&c| (c, self.nodes[c as usize].name.clone(), self.nodes[c as usize].size))
            .collect();
        v.sort_unstable_by(|a, b| b.2.cmp(&a.2));
        v
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", value, UNITS[unit])
    }
}
