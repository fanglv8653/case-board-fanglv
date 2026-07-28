//! 法院申报子进程的安全凭据通道。
//!
//! 凭据只通过一次性 stdin JSON 注入，绝不进入 argv、环境变量、文件或错误文本。

use std::path::Path;
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use zeroize::{Zeroize, Zeroizing};

use crate::proc_util::hide_console_window;

const FORBIDDEN_ARGV: [&str; 3] = ["--account", "--password", "--cookie-dir"];

pub struct FilingCredentials {
    account: String,
    password: String,
}

impl FilingCredentials {
    pub fn new(mut account: String, mut password: String) -> Result<Self, String> {
        if account.is_empty() || account != account.trim() || password.is_empty() {
            account.zeroize();
            password.zeroize();
            return Err("法院申报凭据未配置".to_string());
        }
        Ok(Self { account, password })
    }
}

impl Drop for FilingCredentials {
    fn drop(&mut self) {
        self.account.zeroize();
        self.password.zeroize();
    }
}

#[derive(Clone)]
pub struct FilingRedactor {
    secrets: Vec<String>,
}

impl FilingRedactor {
    fn new(credentials: &FilingCredentials) -> Self {
        let mut secrets = vec![credentials.account.clone(), credentials.password.clone()];
        secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
        Self { secrets }
    }

    pub fn redact(&self, value: &str) -> String {
        self.secrets.iter().fold(value.to_string(), |safe, secret| {
            safe.replace(secret, "[REDACTED]")
        })
    }
}

impl Drop for FilingRedactor {
    fn drop(&mut self) {
        self.secrets.zeroize();
    }
}

fn is_forbidden_flag(arg: &str) -> bool {
    FORBIDDEN_ARGV.iter().any(|forbidden| {
        arg.eq_ignore_ascii_case(forbidden)
            || arg
                .get(..forbidden.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(forbidden))
                && arg.as_bytes().get(forbidden.len()) == Some(&b'=')
    })
}

fn secure_cli_args(
    public_args: &[String],
    credentials: Option<&FilingCredentials>,
) -> Result<Vec<String>, String> {
    if public_args.iter().any(|arg| {
        is_forbidden_flag(arg)
            || credentials.is_some_and(|credentials| {
                arg.contains(&credentials.account) || arg.contains(&credentials.password)
            })
    }) {
        return Err("法院申报启动参数包含禁止的敏感字段".to_string());
    }
    let mut args = vec![
        "-m".to_string(),
        "court_filing_cli".to_string(),
        "--credentials-stdin".to_string(),
    ];
    args.extend_from_slice(public_args);
    Ok(args)
}

pub async fn spawn_with_stdin_credentials(
    program: &str,
    cwd: &Path,
    public_args: &[String],
    credentials: FilingCredentials,
) -> Result<(Child, FilingRedactor), String> {
    let args = secure_cli_args(public_args, Some(&credentials))?;
    let redactor = FilingRedactor::new(&credentials);
    let payload = Zeroizing::new(
        serde_json::to_vec(&serde_json::json!({
            "account": &credentials.account,
            "password": &credentials.password,
        }))
        .map_err(|_| "无法构造法院申报凭据通道".to_string())?,
    );

    let mut command = Command::new(program);
    command
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_console_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| "法院申报运行时启动失败".to_string())?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill().await;
        return Err("法院申报凭据通道不可用".to_string());
    };
    if stdin.write_all(&payload).await.is_err()
        || stdin.write_all(b"\n").await.is_err()
        || stdin.shutdown().await.is_err()
    {
        let _ = child.kill().await;
        return Err("法院申报凭据注入失败".to_string());
    }
    Ok((child, redactor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_process_args_never_accept_secret_flags() {
        let credentials =
            FilingCredentials::new("13800138000".into(), "password-secret".into()).unwrap();
        let args = secure_cli_args(
            &[
                "--filing-type".into(),
                "civil".into(),
                "--output-dir".into(),
                "C:/safe".into(),
            ],
            Some(&credentials),
        )
        .unwrap();
        assert!(args.contains(&"--credentials-stdin".to_string()));
        assert!(!args
            .iter()
            .any(|arg| FORBIDDEN_ARGV.contains(&arg.as_str())));
        for forbidden in FORBIDDEN_ARGV {
            assert!(secure_cli_args(
                &[forbidden.to_string(), "secret".into()],
                Some(&credentials)
            )
            .is_err());
            assert!(secure_cli_args(&[format!("{forbidden}=secret")], Some(&credentials)).is_err());
        }
        assert!(secure_cli_args(
            &["--output-dir".into(), "C:/13800138000".into()],
            Some(&credentials)
        )
        .is_err());
        assert!(secure_cli_args(
            &["--output-dir".into(), "C:/password-secret".into()],
            Some(&credentials)
        )
        .is_err());
    }

    #[test]
    fn errors_are_redacted_for_account_and_password() {
        let credentials =
            FilingCredentials::new("13800138000".into(), "password-secret".into()).unwrap();
        let redactor = FilingRedactor::new(&credentials);
        let safe = redactor.redact("登录 13800138000 失败 password-secret");
        assert_eq!(safe, "登录 [REDACTED] 失败 [REDACTED]");
    }
}
