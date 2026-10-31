#!/usr/bin/env python3
"""Mail capability + realip differential battery (plaintext, oracle-runnable).

Each case records the client-visible wire bytes, so the oracle (real nginx
1.29.6) and the Rust data plane must produce byte-identical greetings and
capability lists. The realip cases record the `auth_http` request the proxy
issued: `Client-IP` proves whether the PROXY protocol source was trusted, while
`Proxy-Protocol-Addr` proves the header itself was parsed.
"""

from __future__ import annotations

import argparse
import base64
import json
import socket
import threading
import time

READ_IDLE = 0.45
CONNECT_TIMEOUT = 10.0
LOGIN_PLAIN = base64.b64encode(b"\x00alice\x00secret").decode("ascii")


class Recorder:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.auth_requests: list[list[str]] = []

    def add_auth(self, headers: list[str]) -> None:
        with self.lock:
            self.auth_requests.append(headers)

    def snapshot(self) -> list[list[str]]:
        with self.lock:
            return [list(headers) for headers in self.auth_requests]


def serve_auth(port: int, recorder: Recorder, backend_port: int) -> socket.socket:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", port))
    listener.listen(16)

    def loop() -> None:
        while True:
            try:
                connection, _ = listener.accept()
            except OSError:
                return
            threading.Thread(
                target=handle_auth,
                args=(connection, recorder, backend_port),
                daemon=True,
            ).start()

    threading.Thread(target=loop, daemon=True).start()
    return listener


def handle_auth(connection: socket.socket, recorder: Recorder, backend_port: int) -> None:
    connection.settimeout(5.0)
    pending = b""
    try:
        while b"\r\n\r\n" not in pending:
            data = connection.recv(4096)
            if not data:
                return
            pending += data
        lines = pending.decode("latin-1").split("\r\n")
        recorder.add_auth([line for line in lines if line])
        response = (
            "HTTP/1.0 200 OK\r\n"
            "Auth-Status: OK\r\n"
            "Auth-User: alice\r\n"
            f"Auth-Server: 127.0.0.1\r\n"
            f"Auth-Port: {backend_port}\r\n"
            "\r\n"
        )
        connection.sendall(response.encode("latin-1"))
    except OSError:
        return
    finally:
        connection.close()


def serve_backend(port: int) -> socket.socket:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", port))
    listener.listen(16)

    def loop() -> None:
        while True:
            try:
                connection, _ = listener.accept()
            except OSError:
                return
            threading.Thread(target=handle_backend, args=(connection,), daemon=True).start()

    threading.Thread(target=loop, daemon=True).start()
    return listener


def handle_backend(connection: socket.socket) -> None:
    connection.settimeout(5.0)
    try:
        connection.sendall(b"220 backend ESMTP\r\n")
        while True:
            data = connection.recv(4096)
            if not data:
                return
            connection.sendall(b"250 backend-ok\r\n")
    except OSError:
        return
    finally:
        connection.close()


def connect(port: int) -> socket.socket:
    deadline = time.monotonic() + CONNECT_TIMEOUT
    while True:
        try:
            return socket.create_connection(("127.0.0.1", port), timeout=4)
        except OSError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)


def read_until_idle(connection: socket.socket, idle: float = READ_IDLE) -> bytes:
    """Read every byte that arrives until the peer goes quiet (no FIN)."""
    received = b""
    connection.settimeout(idle)
    while True:
        try:
            chunk = connection.recv(4096)
        except socket.timeout:
            break
        except OSError:
            break
        if not chunk:
            break
        received += chunk
    return received


def dialog(port: int, commands: list[bytes]) -> list[str]:
    """Connect, read the greeting, then one reply per command."""
    connection = connect(port)
    try:
        transcript = [read_until_idle(connection).decode("latin-1")]
        for command in commands:
            connection.sendall(command + b"\r\n")
            transcript.append(read_until_idle(connection).decode("latin-1"))
        return transcript
    finally:
        connection.close()


def proxy_auth(port: int, recorder: Recorder) -> dict:
    """Send PROXY v1 + EHLO + AUTH PLAIN and record the auth_http request."""
    connection = connect(port)
    try:
        connection.sendall(b"PROXY TCP4 203.0.113.9 127.0.0.1 5555 25\r\n")
        transcript = [read_until_idle(connection).decode("latin-1")]
        connection.sendall(b"EHLO client.example\r\n")
        transcript.append(read_until_idle(connection).decode("latin-1"))
        before = len(recorder.snapshot())
        connection.sendall(f"AUTH PLAIN {LOGIN_PLAIN}\r\n".encode())
        transcript.append(read_until_idle(connection).decode("latin-1"))
        deadline = time.monotonic() + 5.0
        while len(recorder.snapshot()) <= before and time.monotonic() < deadline:
            time.sleep(0.05)
        snapshot = recorder.snapshot()
        auth = snapshot[before] if len(snapshot) > before else []
        return {"client": transcript, "auth_requests": [auth]}
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--smtp-default", type=int, required=True)
    parser.add_argument("--smtp-custom", type=int, required=True)
    parser.add_argument("--imap-default", type=int, required=True)
    parser.add_argument("--imap-custom", type=int, required=True)
    parser.add_argument("--pop3-default", type=int, required=True)
    parser.add_argument("--pop3-custom", type=int, required=True)
    parser.add_argument("--smtp-proxy-trusted", type=int, required=True)
    parser.add_argument("--smtp-proxy-untrusted", type=int, required=True)
    parser.add_argument("--auth-port", type=int, required=True)
    parser.add_argument("--backend-port", type=int, required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    recorder = Recorder()
    auth_listener = serve_auth(args.auth_port, recorder, args.backend_port)
    backend_listener = serve_backend(args.backend_port)

    cases: dict[str, object] = {
        "smtp.ehlo-default": {"client": dialog(args.smtp_default, [b"EHLO client.example"])},
        "smtp.ehlo-custom": {"client": dialog(args.smtp_custom, [b"EHLO client.example"])},
        "imap.capability-default": {"client": dialog(args.imap_default, [b"a1 CAPABILITY"])},
        "imap.capability-custom": {"client": dialog(args.imap_custom, [b"a1 CAPABILITY"])},
        "pop3.capa-default": {"client": dialog(args.pop3_default, [b"CAPA"])},
        "pop3.capa-custom": {"client": dialog(args.pop3_custom, [b"CAPA"])},
        "mail.realip-trusted": proxy_auth(args.smtp_proxy_trusted, recorder),
        "mail.realip-untrusted": proxy_auth(args.smtp_proxy_untrusted, recorder),
    }

    auth_listener.close()
    backend_listener.close()
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {args.out} ({len(cases)} mail cases)")


if __name__ == "__main__":
    main()
