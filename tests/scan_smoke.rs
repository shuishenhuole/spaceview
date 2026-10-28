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
