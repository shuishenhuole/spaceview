//! Windows 专用:处理"文件被其他进程占用导致删除失败"的情况。
//!
//! 流程:
//!  1. `trash::delete` 失败且错误码为共享冲突(文件被占用)时,
//!  2. 用 Windows Restart Manager(RmStartSession/RmRegisterResources/RmGetList)
//!     查出当前占用该文件句柄的进程列表 —— 与资源管理器"显示占用进程"同一机制;
//!  3. 结束这些进程(跳过自身与系统关键进程)后重试删除。
//!
//! 安全策略(宁可不删,不误杀):
//!  - 跳过 SpaceView 自身进程(占用者是自己时无法安全自杀式结束);
//!  - 跳过 PID <= 4(System / smss 等内核级进程);
//!  - 跳过可执行文件位于 %SystemRoot%(通常为 C:\Windows)下的进程,
//!    explorer / svchost / dwm 等系统组件全部落在此规则内。

// 整个模块都是 Win32 FFI 边界,unsafe fn 内的调用不再重复 unsafe 块
#![allow(unsafe_op_in_unsafe_fn)]

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

/// Restart Manager 会话键的自增序号,保证同进程内多次调用键唯一
static RM_SEQ: AtomicU32 = AtomicU32::new(0);

/// 判断删除错误是否为"文件被占用"(共享冲突)。
/// trash 在 Windows 上走 IFileOperation,错误码表现为 HRESULT:
///   0x80270027 = COPYENGINE_E_SHARING_VIOLATION_SRC(日志中显示为 -2144927705)
///   0x80070020 = ERROR_SHARING_VIOLATION(经典共享冲突,-2147024864)
pub fn is_in_use_error(msg: &str) -> bool {
    msg.contains("in use by another process")
        || msg.contains("0x80270027")
        || msg.contains("-2144927705")
        || msg.contains("0x80070020")
        || msg.contains("-2147024864")
}

/// 结束占用 `path` 的所有可安全结束的用户进程,返回被结束的进程名列表。
/// 返回非空即表示已尝试释放锁,调用方应重试删除。
pub fn release_locks(path: &Path) -> Vec<String> {
    // Restart Manager 只支持文件;目录不存在"句柄占用"一说,直接跳过
    if !path.is_file() {
        return Vec::new();
    }
    unsafe {
        // 会话键要求全局唯一字符串,用自增序号生成
        let key = format!(
            "SpaceView-{}",
            RM_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let mut key_w: Vec<u16> = key.encode_utf16().chain([0]).collect();
        let mut session = 0u32;
        if RmStartSession(
            &mut session,
            Some(0),
            PWSTR::from_raw(key_w.as_mut_ptr()),
        ) != ERROR_SUCCESS
        {
            return Vec::new();
        }
        let killed = query_and_kill(session, path);
        // 无论成败都要关闭会话,避免内核侧会话泄漏
        let _ = RmEndSession(session);
        killed
    }
}

/// 在已建立的 Restart Manager 会话内:注册资源 → 查占用进程 → 结束
unsafe fn query_and_kill(session: u32, path: &Path) -> Vec<String> {
    // 路径转宽字符(Windows API 期望 UTF-16 + NUL 结尾)
    let file_w: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    let file_pc = [PCWSTR::from_raw(file_w.as_ptr())];

    if RmRegisterResources(session, Some(&file_pc), None, None) != ERROR_SUCCESS {
        return Vec::new();
    }

    // 两次调用模式:先拿需要的大小,再取完整列表
    let mut needed = 0u32;
    let mut count = 0u32;
    let mut reboot_reasons = 0u32;
    let first = RmGetList(session, &mut needed, &mut count, None, &mut reboot_reasons);
    if first != ERROR_SUCCESS && first != ERROR_MORE_DATA {
        return Vec::new();
    }
    if needed == 0 {
        return Vec::new(); // 没有进程占用(可能锁已被释放)
    }

    let mut infos: Vec<RM_PROCESS_INFO> = vec![std::mem::zeroed(); needed as usize];
    count = needed;
    if RmGetList(
        session,
        &mut needed,
        &mut count,
        Some(infos.as_mut_ptr()),
        &mut reboot_reasons,
    ) != ERROR_SUCCESS
    {
        return Vec::new();
    }

    let self_pid = GetCurrentProcessId();
    let sys_root = std::env::var("SystemRoot")
        .or_else(|_| std::env::var("WINDIR"))
        .unwrap_or_else(|_| r"C:\Windows".to_string())
        .to_lowercase();

    let mut killed = Vec::new();
    for info in infos.iter().take(count as usize) {
        let pid = info.Process.dwProcessId;
        // 应用名是 UTF-16 定长缓冲,按 NUL 截断
        let name_end = info
            .strAppName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.strAppName.len());
        let name = String::from_utf16_lossy(&info.strAppName[..name_end]);
        if name.is_empty() {
            continue;
        }
        // 跳过自身与内核级进程
        if pid == self_pid || pid <= 4 {
            continue;
        }
        // 打开进程拿完整镜像路径,用于排除系统组件
        let Ok(handle) = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            false,
            pid,
        ) else {
            continue; // 无权限(多为系统/受保护进程),跳过
        };
        let mut img = [0u16; 1024];
        let mut img_len = img.len() as u32;
        let is_system = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR::from_raw(img.as_mut_ptr()),
            &mut img_len,
        )
        .is_ok()
            && {
                let img_path = String::from_utf16_lossy(&img[..img_len as usize]);
                img_path.to_lowercase().starts_with(&sys_root)
            };
        if is_system {
            let _ = CloseHandle(handle);
            continue;
        }
        // 终止后立即关闭句柄,避免句柄泄漏
        let terminated = TerminateProcess(handle, 1).is_ok();
        let _ = CloseHandle(handle);
        if terminated {
            killed.push(name);
        }
    }

    if !killed.is_empty() {
        // 给系统一点时间完成句柄清理,否则立即重试仍可能撞上共享冲突
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    killed
}

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, RM_PROCESS_INFO,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use std::os::windows::ffi::OsStrExt;
