use std::fmt;
use std::io::{self, Write};
use std::process::Command;

#[derive(Debug)]
pub enum ExecError {
    NotFound,
    Failed(String),
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::NotFound => write!(f, "command not found"),
            ExecError::Failed(msg) => write!(f, "exec failed: {msg}"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Unix: `exec()` で現プロセスを置き換え、シグナル・stdout/stderr を完全に透過させる。
/// Windows: spawn して終了コードを伝播する。
#[cfg(unix)]
pub fn exec_replace(mut cmd: Command) -> Result<(), ExecError> {
    use std::os::unix::process::CommandExt;
    let err = cmd.exec();
    if err.kind() == std::io::ErrorKind::NotFound {
        Err(ExecError::NotFound)
    } else {
        Err(ExecError::Failed(err.to_string()))
    }
}

#[cfg(windows)]
pub fn exec_replace(mut cmd: Command) -> Result<(), ExecError> {
    let status = cmd.status().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ExecError::NotFound
        } else {
            ExecError::Failed(e.to_string())
        }
    })?;
    std::process::exit(status.code().unwrap_or(1));
}

/// 子プロセスを最後まで動かしたあと、こちら側で `trailer` を stdout に書き出してから
/// 子の終了コードで exit する。pipe 先 (例: `head`) が早く閉じても panic しないよう、
/// BrokenPipe は子の終了コードを保ったまま静かに終了させる。
pub fn run_with_trailer(mut cmd: Command, trailer: &str) -> Result<(), ExecError> {
    let status = cmd.status().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ExecError::NotFound
        } else {
            ExecError::Failed(e.to_string())
        }
    })?;
    let code = status.code().unwrap_or(1);
    let mut out = io::stdout().lock();
    if let Err(e) = out.write_all(trailer.as_bytes()) {
        if e.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(code);
        }
        return Err(ExecError::Failed(e.to_string()));
    }
    let _ = out.flush();
    std::process::exit(code);
}

/// stdout への best-effort 書き出し。BrokenPipe を 0 終了で握りつぶし、それ以外の
/// 失敗は上位に返さず無視する (banner や hint 表示用途)。
pub fn write_or_exit_on_pipe_close(s: &str) {
    let mut out = io::stdout().lock();
    match out.write_all(s.as_bytes()) {
        Ok(_) => {
            let _ = out.flush();
        }
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(_) => {}
    }
}
