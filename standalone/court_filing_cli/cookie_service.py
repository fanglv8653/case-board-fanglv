"""Cookie 加密封套接口。

CLI 不拥有持久密钥，默认严格禁用持久化。只有宿主注入经过审核的 envelope codec
时才允许写二进制密文；明文 JSON 和解密失败一律闭锁。
"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any, Protocol

logger = logging.getLogger("court_filing_cli")


class CookiePersistenceDisabled(RuntimeError):
    pass


class CookieEnvelopeCodec(Protocol):
    def seal(self, plaintext: bytes) -> bytes: ...
    def open(self, envelope: bytes) -> bytes: ...


class CookieService:
    def __init__(
        self,
        storage_path: str | None = None,
        codec: CookieEnvelopeCodec | None = None,
    ) -> None:
        self.storage_path = storage_path
        self.codec = codec

    def load(self, context: Any, storage_path: str | None = None) -> bool:
        path = storage_path or self.storage_path
        if not path or self.codec is None:
            return False
        p = Path(path)
        if not p.exists():
            return False
        try:
            envelope = p.read_bytes()
            if not envelope or envelope.lstrip().startswith((b"{", b"[")):
                raise ValueError("拒绝加载明文 Cookie")
            plaintext = self.codec.open(envelope)
            data = json.loads(plaintext.decode("utf-8"))
            cookies = data.get("cookies") if isinstance(data, dict) else None
            if not isinstance(cookies, list) or not cookies:
                return False
            context.add_cookies(cookies)
            return True
        except Exception:
            logger.warning("Cookie 封套不可用，已闭锁本次会话恢复")
            return False

    def save(self, context: Any, storage_path: str | None = None) -> str:
        path = storage_path or self.storage_path
        if not path or self.codec is None:
            raise CookiePersistenceDisabled("未提供受控加密封套，禁止持久化 Cookie")
        cookies = context.cookies()
        plaintext = json.dumps({"cookies": cookies}, ensure_ascii=False).encode("utf-8")
        envelope = self.codec.seal(plaintext)
        if not envelope or envelope.lstrip().startswith((b"{", b"[")):
            raise CookiePersistenceDisabled("加密封套返回明文，拒绝落盘")
        p = Path(path)
        p.parent.mkdir(parents=True, exist_ok=True)
        temporary = p.with_suffix(p.suffix + ".tmp")
        temporary.write_bytes(envelope)
        temporary.replace(p)
        return str(p)
