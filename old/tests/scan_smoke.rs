//! 扫描引擎冒烟测试:构造小目录树,验证大小聚合与 TopN。

use spaceview::engine::{scanner, tree};
use scanner::{ScanEvent, spawn_scan};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn scan_small_tree() {
    let root: PathBuf = std::env::temp_dir().join(format!("spaceview_smoke_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("a").join("b")).unwrap();
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("x.bin"), vec![0u8; 1000]).unwrap();
    fs::write(root.join("a").join("y.txt"), vec![0u8; 500]).unwrap();
    fs::write(root.join("a").join("b").join("z.log"), vec![0u8; 250]).unwrap();
    fs::write(root.join("c").join("w.bin"), vec![0u8; 100]).unwrap();

    // 扫描器按平台口径计数(macOS 为 st_blocks*512,其余为逻辑长度)
    let disk_size = |p: &std::path::Path| {
        scanner::file_disk_size(&fs::metadata(p).unwrap())
    };
    let (x, y, z, w) = (
        disk_size(&root.join("x.bin")),
        disk_size(&root.join("a").join("y.txt")),
        disk_size(&root.join("a").join("b").join("z.log")),
        disk_size(&root.join("c").join("w.bin")),
    );

    let rx = spawn_scan(
        root.clone(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let result = loop {
        match rx.recv().unwrap() {
            ScanEvent::Tick { .. } => {}
            ScanEvent::Snapshot(_) => {}
            ScanEvent::Done(r) => break r,
            ScanEvent::Cancelled => panic!("不应取消"),
            ScanEvent::Failed(e) => panic!("扫描失败: {e}"),
        }
    };

    println!(
        "total_files={} total_bytes={} nodes={} root_size={}",
        result.total_files,
        result.total_bytes,
        result.tree.nodes.len(),
        result.tree.nodes[tree::ROOT as usize].size
    );
    // 总大小聚合到根
    assert_eq!(result.tree.nodes[tree::ROOT as usize].size, x + y + z + w);
    // a/b = z,a = y + z,c = w
    let a_idx = result
        .tree
        .sorted_children(tree::ROOT)
        .into_iter()
        .find(|(_, n, _)| n == "a")
        .unwrap()
        .0;
    assert_eq!(result.tree.nodes[a_idx as usize].size, y + z);

    // TopN 降序且最大值为四个文件中最大的一个
    assert_eq!(result.top_files[0].size, [x, y, z, w].into_iter().max().unwrap());
    assert!(result.top_files.windows(2).all(|w| w[0].size >= w[1].size));

    // 扩展名统计含 .bin 共 x + w 字节
    let bin = result.ext_stats.iter().find(|e| e.ext == "bin").unwrap();
    assert_eq!(bin.bytes, x + w);
    assert_eq!(bin.count, 2);

    fs::remove_dir_all(&root).unwrap();
}

/// 验证 trash 在本机能正常删除到回收站
#[test]
fn trash_delete_moves_to_recycle_bin() {
    let f = std::env::temp_dir().join(format!("spaceview_trash_test_{}.txt", std::process::id()));
    fs::write(&f, b"qoder test file, safe to delete").unwrap();
    trash::delete(&f).expect("回收站删除失败");
    assert!(!f.exists());
}

/// 后台删除事件:逐项上报 Started / Item,最后以 Done 收尾(用不存在的路径,不产生真实删除)
#[test]
fn background_delete_reports_events() {
    use spaceview::engine::deleter::{self, DeleteEvent};
    use std::sync::Arc;

    let base = std::env::temp_dir().join(format!("spaceview_missing_{}", std::process::id()));
    let p1 = base.join("no_such_1.txt");
    let p2 = base.join("no_such_2.txt");
    let job = deleter::spawn_delete(
        vec![p1.clone(), p2.clone()],
        Arc::new(AtomicBool::new(false)),
        7,
    );
    let mut started = Vec::new();
    let mut errors = 0usize;
    let mut done = false;
    while let Ok(ev) = job.rx.recv() {
        match ev {
            DeleteEvent::Started(p) => started.push(p),
            DeleteEvent::Item(o) => {
                assert!(o.error.is_some(), "不存在的路径应返回错误");
                errors += 1;
            }
            DeleteEvent::Done => {
                done = true;
                break;
            }
        }
    }
    assert!(done, "必须以 Done 事件收尾");
    assert_eq!(started, vec![p1, p2]);
    assert_eq!(errors, 2);
    assert_eq!(job.epoch, 7);
    assert_eq!(job.total, 2);
}

/// 取消标志在启动前置位时,工作线程不处理任何项,但仍以 Done 收尾
#[test]
fn background_delete_can_skip_remaining() {
    use spaceview::engine::deleter::{self, DeleteEvent};
    use std::sync::Arc;

    let cancel = Arc::new(AtomicBool::new(true));
    let paths: Vec<PathBuf> = (0..8)
        .map(|i| std::env::temp_dir().join(format!("spaceview_cancel_{i}.txt")))
        .collect();
    let job = deleter::spawn_delete(paths, Arc::clone(&cancel), 0);
    let mut started = 0usize;
    let mut done = false;
    while let Ok(ev) = job.rx.recv() {
        match ev {
            DeleteEvent::Started(_) => started += 1,
            DeleteEvent::Item(_) => {}
            DeleteEvent::Done => {
                done = true;
                break;
            }
        }
    }
    assert!(done);
    assert_eq!(started, 0);
    assert!(job.cancelled());
}

/// 批量同步:文件回减祖先/扩展名/总量,目录子树清零并保留占位,TopN 剔除子树文件
#[test]
fn apply_deleted_syncs_result_model() {
    use scanner::{ExtStat, FileHit, ScanResult};

    let root = std::env::temp_dir();
    let a_path = root.join("a");
    let b_path = a_path.join("b");
    let x_path = a_path.join("x.bin");
    let z_path = b_path.join("z.log");
    let r_path = root.join("root.bin");

    let mut t = tree::Tree::new(root.clone());
    let a = t.push_node(tree::DirNode {
        parent: Some(tree::ROOT),
        name: "a".into(),
        size: 300,
        file_count: 2,
        children: Vec::new(),
        denied: false,
        deleted: false,
    });
    let b = t.push_node(tree::DirNode {
        parent: Some(a),
        name: "b".into(),
        size: 100,
        file_count: 1,
        children: Vec::new(),
        denied: false,
        deleted: false,
    });
    t.nodes[a as usize].children.push(b);
    t.nodes[tree::ROOT as usize].children.push(a);
    t.nodes[tree::ROOT as usize].size = 400;
    t.nodes[tree::ROOT as usize].file_count = 3;

    let mut result = ScanResult {
        tree: t,
        top_files: vec![
            FileHit {
                path: x_path.clone(),
                size: 200,
                dir_idx: a,
            },
            FileHit {
                path: z_path.clone(),
                size: 100,
                dir_idx: b,
            },
            FileHit {
                path: r_path.clone(),
                size: 100,
                dir_idx: tree::ROOT,
            },
        ],
        ext_stats: vec![
            ExtStat {
                ext: "bin".into(),
                count: 2,
                bytes: 300,
            },
            ExtStat {
                ext: "log".into(),
                count: 1,
                bytes: 100,
            },
        ],
        denied: Vec::new(),
        total_files: 3,
        total_bytes: 400,
    };

    // 一个文件 + 一个目录同时删除
    result.apply_deleted(&[x_path.clone(), b_path.clone()]);

    assert_eq!(result.top_files.len(), 1);
    assert_eq!(result.top_files[0].path, r_path);
    assert_eq!(result.total_bytes, 100);
    assert_eq!(result.total_files, 1);
    assert_eq!(result.tree.nodes[tree::ROOT as usize].size, 100);
    assert_eq!(result.tree.nodes[tree::ROOT as usize].file_count, 1);
    assert_eq!(result.ext_stats[0].count, 1);
    assert_eq!(result.ext_stats[0].bytes, 100);
    assert!(result.tree.nodes[b as usize].deleted);
    assert_eq!(result.tree.nodes[b as usize].size, 0);
    assert!(
        result.tree.nodes[a as usize].children.contains(&b),
        "占位节点应保留在父 children 中"
    );
}

/// 删除同步:子树清零、父 children 保留占位并标记 deleted,祖先回减
#[test]
fn remove_subtree_keeps_tombstone() {
    let mut t = tree::Tree::new(PathBuf::from("C:\\"));
    let a = t.push_node(tree::DirNode {
        parent: Some(tree::ROOT),
        name: "a".into(),
        size: 100,
        file_count: 2,
        children: Vec::new(),
        denied: false,
        deleted: false,
    });
    let b = t.push_node(tree::DirNode {
        parent: Some(a),
        name: "b".into(),
        size: 60,
        file_count: 1,
        children: Vec::new(),
        denied: false,
        deleted: false,
    });
    t.nodes[a as usize].children.push(b);
    t.nodes[tree::ROOT as usize].children.push(a);
    t.nodes[tree::ROOT as usize].size = 100;
    t.nodes[tree::ROOT as usize].file_count = 2;

    t.remove_subtree(a);

    assert!(t.nodes[tree::ROOT as usize].children.contains(&a), "占位节点应保留在父 children 中");
    assert!(t.nodes[a as usize].deleted);
    assert_eq!(t.nodes[a as usize].size, 0);
    assert_eq!(t.nodes[b as usize].size, 0);
    assert_eq!(t.nodes[tree::ROOT as usize].size, 0);
    assert_eq!(t.nodes[tree::ROOT as usize].file_count, 0);
}
