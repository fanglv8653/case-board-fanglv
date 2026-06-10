#!/usr/bin/env python3
"""最小 MCP stdio server stub —— 仅供 CaseBoard 手动验「真子进程往返」用,不进生产。

newline-delimited JSON-RPC 2.0,暴露一个 echo 工具。**故意在每次响应前先吐一条 log 通知**,
考验客户端 read_matching「按 id 匹配、跳过通知/日志」的读循环(手搓传输最容易错的地方)。

用法:被 Rust 测试 `mcp_roundtrip_against_python_stub`(#[ignore])spawn,或手动:
    cargo test mcp_roundtrip -- --ignored
"""
import sys
import json


def send(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def log(msg):
    # 无 id 的通知 —— 客户端必须跳过,别当成响应
    send({"jsonrpc": "2.0", "method": "notifications/message",
          "params": {"level": "info", "data": msg}})


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except Exception:
            continue
        method = req.get("method")
        rid = req.get("id")

        if method == "initialize":
            send({"jsonrpc": "2.0", "id": rid, "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "stub", "version": "0.0.1"},
            }})
        elif method == "notifications/initialized":
            pass  # 通知,不回
        elif method == "tools/list":
            log("about to list tools")  # 先吐通知,考验客户端跳过
            send({"jsonrpc": "2.0", "id": rid, "result": {"tools": [
                {"name": "echo", "description": "回显 msg 参数",
                 "inputSchema": {"type": "object",
                                 "properties": {"msg": {"type": "string"}},
                                 "required": ["msg"]}},
            ]}})
        elif method == "tools/call":
            params = req.get("params", {})
            name = params.get("name")
            args = params.get("arguments", {})
            if name == "echo":
                log("calling echo")  # 先吐通知
                send({"jsonrpc": "2.0", "id": rid, "result": {
                    "content": [{"type": "text", "text": "echo: " + str(args.get("msg", ""))}],
                }})
            else:
                send({"jsonrpc": "2.0", "id": rid,
                      "error": {"code": -32601, "message": "unknown tool"}})
        elif rid is not None:
            send({"jsonrpc": "2.0", "id": rid,
                  "error": {"code": -32601, "message": "unknown method"}})


if __name__ == "__main__":
    main()
