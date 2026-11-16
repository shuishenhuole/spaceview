//! 解析注册表 `UninstallString`,把"命令行字符串"拆成(可执行文件, 参数)。
//!
//! 为什么需要它:以前直接 `cmd /C start "" <原字符串>`,Rust 的 `Command::args`
//! 会把内嵌的双引号转义成 `\"`,而 cmd.exe 不认反斜杠转义 —— 于是路径变成
//! `\C:\Program Files\...\unins000.exe\`,Windows 报
//! "cannot find the file \C:\...\"。改为自行解析后直接 spawn,彻底绕开 cmd。
//!
//! 支持的常见写法:
//!   `"C:\Program Files\App\unins000.exe" /S`          ← 带引号 + 参数
//!   `C:\Program Files\App\unins000.exe /S`            ← 无引号但路径含空格
//!   `MsiExec.exe /X{GUID}`                            ← 裸程序名 + 参数
//!   `"C:\Users\me\AppData\Local\Programs\DLSS"`       ← 仅有目录(个别软件这样登记)

/// 拆分卸载命令行,返回(程序路径, 参数列表)。
/// 参数中的引号会被去掉,引号内的空格不会拆分。
pub fn split_command(raw: &str) -> (String, Vec<String>) {
    let cmd = raw.trim();
    if cmd.is_empty() {
        return (String::new(), Vec::new());
    }

    // 1) 以引号开头:引号内即程序路径
    if let Some(rest) = cmd.strip_prefix('"') {
        return match rest.find('"') {
            Some(end) => (
                rest[..end].to_string(),
                split_args(&rest[end + 1..]),
            ),
            // 引号未闭合:整段当路径,避免把路径切断
            None => (rest.trim().to_string(), Vec::new()),
        };
    }

    // 2) 无引号:路径可能含空格,从右往左找"最长的、真实存在的文件路径"
    //    (存在的路径越长越可信,可正确处理 `C:\Program Files\A\unins000.exe /S`)
    let mut best: Option<(usize, String)> = None;
    for (idx, _) in cmd.match_indices(' ') {
        let candidate = &cmd[..idx];
        if candidate.len() > 3 && std::path::Path::new(candidate).is_file() {
            best = Some((idx, candidate.to_string()));
        }
    }
    if let Some((idx, program)) = best {
        return (program, split_args(&cmd[idx + 1..]));
    }

    // 3) 兜底:按空白切分,第一段当程序名(如 MsiExec.exe)
    let mut parts = split_args(cmd);
    if parts.is_empty() {
        return (cmd.to_string(), Vec::new());
    }
    let program = parts.remove(0);
    (program, parts)
}

/// 按空白切分参数,保留引号内的空格并去掉引号
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in s.chars() {
        match ch {
            '"' => in_quote = !in_quote,
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
