//! 引擎层单元测试:目录大小聚合、TopN、扩展名统计、删除事件与模型同步。
//! 全部使用临时目录或不存在的路径,不会真的往回收站删除文件。

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use spaceview_lib::engine::deleter::{self, DeleteEvent};
use spaceview_lib::engine::scanner::{self, ExtStat, FileHit, ScanEvent, ScanResult};
use spaceview_lib::engine::tree::{self, DirNode};
use spaceview_lib::engine::uninstall;

fn flags() -> (Arc<AtomicBool>, Arc<AtomicBool>) {
    (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

fn scan(root: PathBuf) -> Box<ScanResult> {
    let (cancel, paused) = flags();
    let mut events = Vec::new();
    scanner::run_scan(root, cancel, paused, |ev| events.push(ev)).expect("scan failed");
    events
        .into_iter()
        .find_map(|e| match e {
            ScanEvent::Done(r) => Some(r),
            _ => None,
        })
        .expect("missing Done event")
}

#[test]
fn scan_small_tree() {
    let root: PathBuf =
        std::env::temp_dir().join(format!("spaceview_smoke_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("a").join("b")).unwrap();
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("x.bin"), vec![0u8; 1000]).unwrap();
    fs::write(root.join("a").join("y.txt"), vec![0u8; 500]).unwrap();
    fs::write(root.join("a").join("b").join("z.log"), vec![0u8; 250]).unwrap();
    fs::write(root.join("c").join("w.bin"), vec![0u8; 100]).unwrap();

    // 扫描器按平台口径计数(macOS 为 st_blocks*512,其余为逻辑长度)
    let disk_size = |p: &std::path::Path| scanner::file_disk_size(&fs::metadata(p).unwrap());
    let (x, y, z, w) = (
        disk_size(&root.join("x.bin")),
        disk_size(&root.join("a").join("y.txt")),
        disk_size(&root.join("a").join("b").join("z.log")),
        disk_size(&root.join("c").join("w.bin")),
    );

    let result = scan(root.clone());

    // 总大小聚合到根
    assert_eq!(result.tree.nodes[tree::ROOT as usize].size, x + y + z + w);
    // a = y + z
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

/// 后台删除事件:逐项上报 Started / Item,汇总正确的成功/失败数
#[test]
fn background_delete_reports_events() {
    let base = std::env::temp_dir().join(format!("spaceview_missing_{}", std::process::id()));
    let p1 = base.join("no_such_1.txt");
    let p2 = base.join("no_such_2.txt");
    let (cancel, _) = flags();
    let mut started = Vec::new();
    let mut errors = 0usize;
    let summary = deleter::run_delete(vec![p1.clone(), p2.clone()], cancel, |ev| match ev {
        DeleteEvent::Started(p) => started.push(p),
        DeleteEvent::Item(o) => {
            assert!(o.error.is_some(), "不存在的路径应返回错误");
            errors += 1;
        }
    });
    // 并行工作线程下事件顺序不固定,只校验集合与计数
    assert_eq!(started.len(), 2);
    assert!(started.contains(&p1) && started.contains(&p2));
    assert_eq!(errors, 2);
    assert_eq!(summary.total, 2);
    assert_eq!(summary.done, 2);
    assert_eq!(summary.ok, 0);
    assert_eq!(summary.errors.len(), 2);
}

/// 取消标志在启动前置位时,工作线程不处理任何项
#[test]
fn background_delete_can_skip_remaining() {
    let cancel = Arc::new(AtomicBool::new(true));
    let paths: Vec<PathBuf> = (0..8)
        .map(|i| std::env::temp_dir().join(format!("spaceview_cancel_{i}.txt")))
        .collect();
    let mut started = 0usize;
    let summary = deleter::run_delete(paths, Arc::clone(&cancel), |ev| {
        if matches!(ev, DeleteEvent::Started(_)) {
            started += 1;
        }
    });
    assert_eq!(started, 0);
    assert_eq!(summary.done, 0);
    assert!(summary.cancelled);
}

/// 批量同步:文件回减祖先/扩展名/总量,目录子树清零并保留占位,TopN 剔除子树文件
#[test]
fn apply_deleted_syncs_result_model() {
    let root = std::env::temp_dir();
    let a_path = root.join("a");
    let b_path = a_path.join("b");
    let x_path = a_path.join("x.bin");
    let z_path = b_path.join("z.log");
    let r_path = root.join("root.bin");

    let mut t = tree::Tree::new(root.clone());
    let a = t.push_node(DirNode {
        parent: Some(tree::ROOT),
        name: "a".into(),
        size: 300,
        file_count: 2,
        children: Vec::new(),
        denied: false,
        deleted: false,
    });
    let b = t.push_node(DirNode {
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
    let a = t.push_node(DirNode {
        parent: Some(tree::ROOT),
        name: "a".into(),
        size: 100,
        file_count: 2,
        children: Vec::new(),
        denied: false,
        deleted: false,
    });
    let b = t.push_node(DirNode {
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

    assert!(t.nodes[tree::ROOT as usize].children.contains(&a));
    assert!(t.nodes[a as usize].deleted);
    assert_eq!(t.nodes[a as usize].size, 0);
    assert_eq!(t.nodes[b as usize].size, 0);
    assert_eq!(t.nodes[tree::ROOT as usize].size, 0);
    assert_eq!(t.nodes[tree::ROOT as usize].file_count, 0);
}

// ---------- 卸载命令行解析(修复路径前多出反斜杠、找不到文件的问题) ----------

#[test]
fn uninstall_quoted_path_with_args() {
    // 常见写法:程序路径带引号 + 静默安装参数
    let (program, args) = uninstall::split_command(
        r#""C:\Program Files\Concat\unins000.exe" /SILENT /NORESTART"#,
    );
    assert_eq!(program, r"C:\Program Files\Concat\unins000.exe");
    assert_eq!(args, vec!["/SILENT", "/NORESTART"]);
}

#[test]
fn uninstall_bare_program_with_args() {
    // 裸程序名(msiexec 等),不在当前目录也照样解析
    let (program, args) = uninstall::split_command("MsiExec.exe /X{1234-5678}");
    assert_eq!(program, "MsiExec.exe");
    assert_eq!(args, vec!["/X{1234-5678}"]);
}

#[test]
fn uninstall_unquoted_path_with_spaces() {
    // 无引号但路径含空格:识别真实存在的文件路径作为程序(用临时文件模拟)
    let dir = std::env::temp_dir().join(format!("spaceview_uni_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("unins000.exe");
    fs::write(&exe, b"stub").unwrap();

    let raw = format!("{} /S", exe.display());
    let (program, args) = uninstall::split_command(&raw);
    assert_eq!(PathBuf::from(&program), exe);
    assert_eq!(args, vec!["/S"]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn uninstall_directory_only_command() {
    // 个别软件把"安装目录"直接登记为卸载项,解析后应原样得到该目录
    let (program, args) =
        uninstall::split_command(r#""C:\Users\me\AppData\Local\Programs\DLSS""#);
    assert_eq!(program, r"C:\Users\me\AppData\Local\Programs\DLSS");
    assert!(args.is_empty());
}

