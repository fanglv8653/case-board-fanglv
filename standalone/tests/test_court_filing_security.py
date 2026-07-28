import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from court_filing_cli.cli import build_parser
from court_filing_cli.cookie_service import CookiePersistenceDisabled, CookieService
from court_filing_cli.progress import emit
from court_filing_cli.secrets import (
    read_secrets_from_stdin,
    redact_value,
    register_secrets,
)


class _Context:
    def __init__(self):
        self.injected = []

    def cookies(self):
        return [{"name": "SESSION", "value": "plaintext-session-secret"}]

    def add_cookies(self, cookies):
        self.injected.extend(cookies)


class _XorCodec:
    def seal(self, plaintext: bytes) -> bytes:
        return b"CFEN1" + bytes(value ^ 0xA5 for value in plaintext)

    def open(self, envelope: bytes) -> bytes:
        if not envelope.startswith(b"CFEN1"):
            raise ValueError("bad envelope")
        return bytes(value ^ 0xA5 for value in envelope[5:])


class _BrokenCodec:
    def seal(self, plaintext: bytes) -> bytes:
        return b"CFEN1broken"

    def open(self, envelope: bytes) -> bytes:
        raise ValueError("authentication failed")


class CourtFilingSecurityTests(unittest.TestCase):
    def test_parser_has_no_secret_or_cookie_path_arguments(self):
        options = {
            option
            for action in build_parser()._actions
            for option in action.option_strings
        }
        self.assertNotIn("--account", options)
        self.assertNotIn("--password", options)
        self.assertNotIn("--cookie-dir", options)
        self.assertIn("--credentials-stdin", options)

    def test_login_flow_never_screenshots_after_credentials_are_filled(self):
        source = (
            Path(__file__).parents[1]
            / "court_filing_cli"
            / "sites"
            / "court_zxfw.py"
        ).read_text(encoding="utf-8")
        self.assertNotIn('self._save_screenshot("03_credentials_filled")', source)
        self.assertNotIn('self._save_screenshot("error_login_failed")', source)
        self.assertNotIn('self._save_screenshot(f"04_captcha_filled', source)
        self.assertNotIn('self._save_screenshot(f"05_after_login', source)

    def test_credentials_are_one_line_stdin_only_and_progress_is_redacted(self):
        payload = b'{"account":"13800138000","password":"very-secret"}\n'
        stdin = io.TextIOWrapper(io.BytesIO(payload), encoding="utf-8")
        output = io.StringIO()
        with patch.object(sys, "stdin", stdin), patch.object(sys, "stdout", output):
            secrets = read_secrets_from_stdin()
            emit(
                "system",
                "test",
                f"account={secrets.account}, password={secrets.password}",
                detail={"nested": secrets.password},
            )
        rendered = output.getvalue()
        self.assertNotIn("13800138000", rendered)
        self.assertNotIn("very-secret", rendered)
        self.assertIn("[REDACTED]", rendered)

    def test_credentials_reject_trailing_second_line(self):
        payload = (
            b'{"account":"13800138000","password":"very-secret"}\n'
            b'{"ignored":"must-not-be-accepted"}\n'
        )
        stdin = io.TextIOWrapper(io.BytesIO(payload), encoding="utf-8")
        with patch.object(sys, "stdin", stdin), self.assertRaises(ValueError):
            read_secrets_from_stdin()

    def test_recursive_redaction_covers_dictionary_keys(self):
        register_secrets("secret-as-key")
        rendered = json.dumps(
            redact_value({"secret-as-key": {"nested": "secret-as-key"}})
        )
        self.assertNotIn("secret-as-key", rendered)
        self.assertIn("[REDACTED]", rendered)

    def test_cookie_plaintext_never_lands_without_envelope_codec(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cookies.bin"
            service = CookieService(str(path))
            with self.assertRaises(CookiePersistenceDisabled):
                service.save(_Context())
            self.assertFalse(path.exists())

    def test_envelope_contains_no_plain_cookie_and_can_round_trip(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cookies.bin"
            service = CookieService(str(path), _XorCodec())
            service.save(_Context())
            raw = path.read_bytes()
            self.assertNotIn(b"plaintext-session-secret", raw)
            self.assertFalse(raw.lstrip().startswith(b"{"))
            restored = _Context()
            self.assertTrue(service.load(restored))
            self.assertEqual(restored.injected[0]["name"], "SESSION")

    def test_decryption_failure_is_fail_closed_without_cookie_injection(self):
        register_secrets("authentication failed")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cookies.bin"
            path.write_bytes(b"CFEN1corrupt")
            context = _Context()
            service = CookieService(str(path), _BrokenCodec())
            self.assertFalse(service.load(context))
            self.assertEqual(context.injected, [])


if __name__ == "__main__":
    unittest.main()
