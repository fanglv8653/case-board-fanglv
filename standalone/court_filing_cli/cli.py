"""法院「一张网」在线立案 CLI 主入口。

用法:
    python -m court_filing_cli \\
        --credentials-stdin \\
        --filing-type civil \\
        --case-data /path/to/case_data.json \\
        --materials /path/to/materials.json \\
        --output-dir /tmp/court_filing/job1 \\
        [--headless] \\
        [--captcha-mode auto] \\
        [--save-screenshot]

退出码:
    0 = 已到预览页（未提交）
    1 = 失败（详情见 stderr + 最后一条 stdout 事件）
    2 = 参数错误
"""

import argparse
import json
import logging
import os
import sys
import time

from court_filing_cli.progress import EXIT_SUCCESS, EXIT_FAILURE, EXIT_ARG_ERROR, emit, emit_result
from court_filing_cli.schemas import CaseData, load_case_data, load_materials, validate_case_data
from court_filing_cli.secrets import (
    FilingSecrets,
    SecretRedactionFilter,
    read_secrets_from_stdin,
    redact_text,
)


# ────────────────────────── Logging 配置 ──────────────────────────

def _configure_logging(level: str = "INFO") -> None:
    """配置 logging：所有日志输出到 stderr（stdout 只写 JSONL，给 Rust 读）。"""
    root = logging.getLogger("court_filing_cli")
    root.setLevel(getattr(logging, level.upper(), logging.INFO))
    handler = logging.StreamHandler(sys.stderr)
    handler.setFormatter(logging.Formatter("[%(asctime)s] %(name)s %(levelname)s: %(message)s", datefmt="%H:%M:%S"))
    handler.addFilter(SecretRedactionFilter())
    root.addHandler(handler)

    # 防止库的日志泄露到 stdout
    for lib in ("playwright", "ddddocr", "urllib3", "httpx", "httpcore"):
        lib_logger = logging.getLogger(lib)
        lib_logger.setLevel(logging.WARNING)
        lib_logger.addHandler(handler)


# ────────────────────────── 主流程 ──────────────────────────

def run_login(args: argparse.Namespace, secrets: FilingSecrets) -> None:
    """M1：仅登录。CLI 默认不持久化 Cookie。"""
    from court_filing_cli.browser import create_browser
    from court_filing_cli.sites.court_zxfw import CourtZxfwService
    from court_filing_cli.progress import emit

    output_dir = args.output_dir
    debug_dir = output_dir if args.save_screenshot else None

    # 构建验证码识别器
    from court_filing_cli.captcha_manual import AutoDegradingRecognizer, ManualCaptchaRecognizer
    if args.captcha_mode == "manual":
        captcha_recognizer = ManualCaptchaRecognizer(output_dir=output_dir, task_id="login-test")
    else:
        captcha_recognizer = AutoDegradingRecognizer(output_dir=output_dir, max_auto_attempts=3)

    emit("system", "cli.started", f"CLI 启动 (login-only mode)", captcha_mode=args.captcha_mode)

    try:
        with create_browser(headless=args.headless) as (page, context):
            service = CourtZxfwService(
                page=page,
                context=context,
                cookie_service=None,
                captcha_recognizer=captcha_recognizer,
                debug_dir=debug_dir,
            )

            max_retries = 10 if args.captcha_mode == "auto" else 3
            emit("login", "login.start", "正在登录一张网...")
            result = service.login(
                account=secrets.account,
                password=secrets.password,
                max_captcha_retries=max_retries,
                save_debug=args.save_screenshot,
            )

            if result.get("success"):
                msg = result.get("message", "登录成功")
                if result.get("used_cookie"):
                    emit("login", "login.success", msg, used_cookie=True)
                else:
                    emit("login", "login.success", msg)
                emit_result(True, msg, url=result.get("url"))
            else:
                msg = result.get("message", "登录失败")
                emit("login", "login.failed", msg, level="error")
                emit_result(False, msg)

    except Exception as e:
        safe_error = redact_text(e)
        emit("system", "cli.error", f"CLI 异常: {safe_error}", level="error")
        emit_result(False, f"CLI 异常: {safe_error}")
        sys.exit(EXIT_FAILURE)


def run_filing(args: argparse.Namespace, secrets: FilingSecrets) -> None:
    """立案主流程（登录 + 民事6步/执行5步 → 到预览页）。"""
    from court_filing_cli.runner import run_filing as _run
    from court_filing_cli.schemas import load_case_data, load_materials, validate_case_data

    emit("system", "cli.started", "CLI 启动 (filing mode)")

    case_data = load_case_data(args.case_data)
    errors = validate_case_data(case_data)
    if errors:
        for err in errors:
            emit("system", "cli.error", f"校验失败: {err}", level="error")
        sys.exit(EXIT_ARG_ERROR)

    materials = {}
    if args.materials:
        materials = load_materials(args.materials)

    emit("system", "cli.info", f"立案类型: {case_data.filing_type}, 法院: {case_data.court_name}")
    emit("system", "cli.info", f"原告: {len(case_data.plaintiffs)}, 被告: {len(case_data.defendants)}, 律师: {len(case_data.agents)}")
    emit("system", "cli.info", f"材料槽位: {list(materials.keys())}")

    result = _run(
        case_data=case_data,
        materials=materials,
        account=secrets.account,
        password=secrets.password,
        output_dir=args.output_dir,
        headless=args.headless,
        save_screenshot=args.save_screenshot,
        captcha_mode=args.captcha_mode,
    )

    success = result.get("success", False)
    msg = result.get("message", "")
    emit_result(success, msg, timing=result.get("timing"), url=result.get("url"))
    sys.exit(EXIT_SUCCESS if success else EXIT_FAILURE)


# ────────────────────────── CLI 参数 ──────────────────────────

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="court_filing_cli",
        description="法院「一张网」在线立案 CLI（法穿独立抽取版）",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="示例（凭据 JSON 由父进程写 stdin，不得写命令行）:\n"
               "  python -m court_filing_cli --credentials-stdin "
               "--output-dir /tmp/test\n"
               "  python -m court_filing_cli --credentials-stdin "
               "--filing-type civil --case-data case.json --materials mats.json "
               "--output-dir /tmp/job1\n",
    )

    parser.add_argument(
        "--credentials-stdin",
        action="store_true",
        required=True,
        help="从 stdin 读取一次性账号/密码 JSON（唯一支持的凭据入口）",
    )

    # 模式
    parser.add_argument("--login-only", action="store_true",
                        help="仅登录验证（不立案，M1 测试用）")
    parser.add_argument("--filing-type", choices=["civil", "execution"], default="civil",
                        help="立案类型：civil=民事一审, execution=申请执行 (默认 civil)")
    parser.add_argument("--case-data", help="case_data.json 路径（filing 模式必填）")
    parser.add_argument("--materials", help="materials.json 路径（filing 模式可选）")

    # 输出
    parser.add_argument("--output-dir", required=True, help="输出目录（截图/进度/日志）")

    # 浏览器
    parser.add_argument("--headless", action="store_true",
                        help="无头模式（默认关闭，肉眼可见浏览器便于人工兜底验证码）")
    parser.add_argument("--save-screenshot", action="store_true",
                        help="保存每步截图到 output_dir（调试用）")

    # 验证码
    parser.add_argument("--captcha-mode", choices=["auto", "manual"], default="auto",
                        help="验证码模式：auto=ddddocr 自动（失败降级 manual）, manual=直接人工 (默认 auto)")

    # 日志
    parser.add_argument("--log-level", default="INFO", choices=["DEBUG", "INFO", "WARNING", "ERROR"],
                        help="日志级别 (默认 INFO)")
    parser.add_argument("--version", action="version", version="%(prog)s 0.1.0")

    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    _configure_logging(args.log_level)
    try:
        secrets = read_secrets_from_stdin()
    except ValueError as error:
        emit("system", "cli.credentials_rejected", redact_text(error), level="error")
        sys.exit(EXIT_ARG_ERROR)
    try:
        os.makedirs(args.output_dir, exist_ok=True)
        emit(
            "system",
            "cli.params",
            f"headless={args.headless}, captcha_mode={args.captcha_mode}, "
            f"output_dir={args.output_dir}, cookie_persistence=disabled",
        )
        if args.login_only:
            run_login(args, secrets)
        else:
            if not args.case_data:
                emit("system", "cli.error", "filing 模式需要 --case-data", level="error")
                sys.exit(EXIT_ARG_ERROR)
            run_filing(args, secrets)
    except SystemExit:
        raise
    except Exception as error:
        safe_error = redact_text(error)
        emit("system", "cli.error", f"CLI 异常: {safe_error}", level="error")
        emit_result(False, f"CLI 异常: {safe_error}")
        sys.exit(EXIT_FAILURE)


if __name__ == "__main__":
    main()
