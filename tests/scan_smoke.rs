//! 扫描引擎冒烟测试:构造小目录树,验证大小聚合与 TopN。

mod engine {
    #[path = "../../src/engine/tree.rs"]
    pub mod tree;
    #[path = "../../src/engine/scanner.rs"]
    pub mod scanner;
}

use engine::scanner::{ScanEvent, spawn_scan};
use engine::tree;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn scan_small_tree() {
    let root: PathBuf = std::env::temp_dir().join(format!("analyse_smoke_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("a\\b")).unwrap();
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("x.bin"), vec![0u8; 1000]).unwrap();
    fs::write(root.join("a\\y.txt"), vec![0u8; 500]).unwrap();
    fs::write(root.join("a\\b\\z.log"), vec![0u8; 250]).unwrap();
    fs::write(root.join("c\\w.bin"), vec![0u8; 100]).unwrap();

    let rx = spawn_scan(root.clone(), Arc::new(AtomicBool::new(false)));
    let result = loop {
        match rx.recv().unwrap() {
            ScanEvent::Tick { .. } => {}
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
    assert_eq!(result.tree.nodes[tree::ROOT as usize].size, 1850);
    // a\b = 250,a = 750,c = 100
    let a_idx = result
        .tree
        .sorted_children(tree::ROOT)
        .into_iter()
        .find(|(_, n, _)| n == "a")
        .unwrap()
        .0;
    assert_eq!(result.tree.nodes[a_idx as usize].size, 750);

    // TopN 降序且最大值 1000
    assert_eq!(result.top_files[0].size, 1000);
    assert!(result.top_files.windows(2).all(|w| w[0].size >= w[1].size));

    // 扩展名统计含 .bin 共 1100 字节
    let bin = result.ext_stats.iter().find(|e| e.ext == "bin").unwrap();
    assert_eq!(bin.bytes, 1100);
    assert_eq!(bin.count, 2);

    fs::remove_dir_all(&root).unwrap();
}

/// 验证 trash 在本机能正常删除到回收站
#[test]
fn trash_delete_moves_to_recycle_bin() {
    let f = std::env::temp_dir().join(format!("analyse_trash_test_{}.txt", std::process::id()));
    fs::write(&f, b"qoder test file, safe to delete").unwrap();
    trash::delete(&f).expect("回收站删除失败");
    assert!(!f.exists());
}
