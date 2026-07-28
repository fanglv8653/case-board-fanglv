"""一次性 stdin 凭据与全链路脱敏。

账号和密码只允许由父进程通过 stdin 写入一行 JSON，不接受 argv、环境变量或文件。
"""

from __future__ import annotations

import json
import logging
import sys
from dataclasses import dataclass
from typing import Any

MAX_SECRET_PAYLOAD_BYTES = 16 * 1024
REDACTED = "[REDACTED]"
_secrets: set[str] = set()


@dataclass(frozen=True)
class FilingSecrets:
    account: str
    password: str


def read_secrets_from_stdin() -> FilingSecrets:
    """读取且仅读取一行凭据 JSON；异常时闭锁，不回显输入。"""
    if sys.stdin.isatty():
        raise ValueError("凭据必须由受控父进程通过 stdin 注入")
    raw = sys.stdin.buffer.read(MAX_SECRET_PAYLOAD_BYTES + 1)
    if (
        not raw
        or len(raw) > MAX_SECRET_PAYLOAD_BYTES
        or not raw.endswith(b"\n")
        or raw.count(b"\n") != 1
    ):
        raise ValueError("凭据输入缺失、过长或不是单行协议")
    try:
        payload = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError("凭据输入不是有效 JSON") from exc
    if not isinstance(payload, dict) or set(payload) != {"account", "password"}:
        raise ValueError("凭据输入字段不符合协议")
    account = payload.get("account")
    password = payload.get("password")
    if not isinstance(account, str) or not account.strip():
        raise ValueError("账号不能为空")
    if not isinstance(password, str) or not password:
        raise ValueError("密码不能为空")
    secrets = FilingSecrets(account=account.strip(), password=password)
    register_secrets(secrets.account, secrets.password)
    return secrets


def register_secrets(*values: str) -> None:
    _secrets.update(value for value in values if value)


def redact_text(value: object) -> str:
    text = str(value)
    for secret in sorted(_secrets, key=len, reverse=True):
        text = text.replace(secret, REDACTED)
    return text


def redact_value(value: Any) -> Any:
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, dict):
        return {redact_text(key): redact_value(item) for key, item in value.items()}
    if isinstance(value, list):
        return [redact_value(item) for item in value]
    if isinstance(value, tuple):
        return tuple(redact_value(item) for item in value)
    return value


class SecretRedactionFilter(logging.Filter):
    """在日志格式化前消除消息、参数和异常文本中的已登记凭据。"""

    def filter(self, record: logging.LogRecord) -> bool:
        record.msg = redact_text(record.getMessage())
        record.args = ()
        if record.exc_info:
            record.exc_text = REDACTED
            record.exc_info = None
        return True
