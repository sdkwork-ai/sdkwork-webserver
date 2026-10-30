"""Mail-proxy differential battery: real nginx `mail {}` vs the Rust data plane.

One script, two endpoints. For every case it records the three observable wire
flows the mail proxy owns:

- the client-visible byte transcript (TLS cases record the decrypted bytes, so
  the TLS handshake itself is not compared);
- the lines the mock backend received for that case;
- the headers of the auth_http request the proxy issued.

The mock auth_http service and the mock POP3/IMAP/SMTP backends are
deterministic functions of the request, so any difference is a real behavioural
difference. Ports are parameterised (the oracle and the gateway never share a
socket); `Auth-SSL-Cipher` is normalised because nginx reports an OpenSSL cipher
name while rustls reports its own suite name (the negotiated suite is proven
separately by the TLS handshake, not by the label).
"""

from __future__ import annotations

import argparse
import base64
import json
import socket
import ssl
import threading
import time

READ_IDLE = 0.45
CONNECT_TIMEOUT = 10.0

LOGIN_PLAIN = base64.b64encode(b"\x00alice\x00secret").decode("ascii")
LOGIN_PLAIN_BAD = base64.b64encode(b"\x00bad\x00secret").decode("ascii")


class Recorder:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.auth_requests: list[list[str]] = []
        self.backend_sessions: dict[str, list[list[str]]] = {
            "pop3": [],
            "imap": [],
            "smtp": [],
        }

    def add_auth(self, headers: list[str]) -> None:
        with self.lock:
            self.auth_requests.append(headers)

    def add_backend_line(self, protocol: str, line: str) -> None:
        with self.lock:
            sessions = self.backend_sessions[protocol]
            if not sessions:
                sessions.append([])
            sessions[-1].append(line)

    def begin_backend_session(self, protocol: str) -> None:
        with self.lock:
            self.backend_sessions[protocol].append([])

    def snapshot(self) -> dict:
        with self.lock:
            return {
                "auth_requests": [list(headers) for headers in self.auth_requests],
                "backend_sessions": {
                    protocol: [list(lines) for lines in sessions]
                    for protocol, sessions in self.backend_sessions.items()
                },
            }

    def clear_backend_sessions(self) -> None:
        with self.lock:
            for sessions in self.backend_sessions.values():
                sessions.clear()


def _serve(port: int, handler, greeting: bytes | None, recorder: Recorder, protocol: str | None):
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
                target=serve_connection,
                args=(connection, handler, greeting, recorder, protocol),
                daemon=True,
            ).start()

    threading.Thread(target=loop, daemon=True).start()
    return listener


def serve_connection(connection, handler, greeting, recorder, protocol) -> None:
    connection.settimeout(15.0)
    pending = b""
    try:
        if protocol is not None:
            recorder.begin_backend_session(protocol)
        if greeting is not None:
            connection.sendall(greeting)
        while True:
            data = connection.recv(4096)
            if not data:
                return
            pending += data
            while b"\r\n" in pending:
                line, pending = pending.split(b"\r\n", 1)
                text = line.decode("latin-1")
                if protocol is not None:
                    recorder.add_backend_line(protocol, text)
                reply = handler(text)
                if reply is None:
                    connection.close()
                    return
                if reply:
                    connection.sendall(reply)
    except OSError:
        return
    finally:
        try:
            connection.close()
        except OSError:
            pass


def _auth_http_handler(backend_ports: dict[str, int]):
    state = {"wait_served": False}

    def handle(headers: list[str]):
        values = {}
        for header in headers[1:]:
            if ":" in header:
                name, value = header.split(":", 1)
                values[name.strip().lower()] = value.strip()
        protocol = values.get("auth-protocol", "")
        user = values.get("auth-user", "")
        if user == "bad":
            extra = "Auth-Error-Code: 535 5.7.0\r\n" if protocol == "smtp" else ""
            return (
                "HTTP/1.0 200 OK\r\nAuth-Status: invalid credentials\r\n"
                + extra
                + "\r\n"
            ).encode()
        if user == "wait" and not state["wait_served"]:
            state["wait_served"] = True
            return b"HTTP/1.0 200 OK\r\nAuth-Status: WAIT\r\nAuth-Wait: 0\r\n\r\n"
        port = backend_ports[protocol]
        return (
            "HTTP/1.0 200 OK\r\nAuth-Status: OK\r\nAuth-Server: 127.0.0.1\r\n"
            f"Auth-Port: {port}\r\n\r\n"
        ).encode()

    return handle


def start_auth_http(port: int, backend_ports: dict[str, int], recorder: Recorder):
    handler = _auth_http_handler(backend_ports)

    def request_handler(head: bytes) -> bytes:
        headers = head.decode("latin-1").split("\r\n")
        recorder.add_auth(headers)
        return handler(headers)

    def serve_one(connection) -> None:
        connection.settimeout(15.0)
        pending = b""
        try:
            while b"\r\n\r\n" not in pending:
                data = connection.recv(4096)
                if not data:
                    return
                pending += data
            head = pending.split(b"\r\n\r\n")[0]
            connection.sendall(request_handler(head))
        except OSError:
            return
        finally:
            connection.close()

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
            threading.Thread(target=serve_one, args=(connection,), daemon=True).start()

    threading.Thread(target=loop, daemon=True).start()
    return listener


def pop3_backend(line: str) -> bytes | None:
    verb = line.split(" ")[0].upper()
    if verb == "USER":
        return b"+OK user accepted\r\n"
    if verb == "PASS":
        return b"+OK logged in\r\n"
    if verb == "NOOP":
        return b"+OK echo NOOP\r\n"
    if verb == "QUIT":
        return b"+OK bye\r\n"
    return None


def imap_backend(line: str) -> bytes | None:
    parts = line.split(" ")
    tag = parts[0] if parts else "a1"
    if len(parts) > 1 and parts[1].upper() == "LOGIN":
        return b"+ go\r\n"
    if line.startswith("alice {"):
        return b"+ next\r\n"
    if line == "secret":
        return b"a1 OK logged in\r\n"
    if "NOOP" in line.upper():
        return f"{tag} OK echo\r\n".encode()
    if "LOGOUT" in line.upper():
        return b"* BYE\r\n" + f"{tag} OK\r\n".encode()
    return None


def smtp_backend(line: str) -> bytes | None:
    upper = line.upper()
    if upper.startswith("EHLO"):
        return b"250-backend.example\r\n250 XCLIENT\r\n"
    if upper.startswith("HELO"):
        return b"250 backend.example\r\n"
    if upper.startswith("XCLIENT"):
        return b"250 xclient ok\r\n"
    if upper.startswith("AUTH PLAIN"):
        return b"235 2.0.0 ok\r\n"
    if upper.startswith("QUIT"):
        return b"221 bye\r\n"
    return None


def start_backends(ports: dict[str, int], recorder: Recorder) -> list[socket.socket]:
    return [
        _serve(ports["pop3"], pop3_backend, b"+OK backend POP3 ready\r\n", recorder, "pop3"),
        _serve(ports["imap"], imap_backend, b"* OK backend IMAP ready\r\n", recorder, "imap"),
        _serve(ports["smtp"], smtp_backend, b"220 backend.example ESMTP\r\n", recorder, "smtp"),
    ]


def connect(port: int) -> socket.socket:
    deadline = time.time() + CONNECT_TIMEOUT
    while True:
        try:
            return socket.create_connection(("127.0.0.1", port), timeout=CONNECT_TIMEOUT)
        except OSError:
            if time.time() > deadline:
                raise
            time.sleep(0.05)


def read_until_idle(connection, idle: float = READ_IDLE) -> bytes:
    collected = b""
    connection.settimeout(idle)
    while True:
        try:
            data = connection.recv(65536)
        except socket.timeout:
            return collected
        except OSError:
            return collected
        if not data:
            return collected
        collected += data


def tls_context() -> ssl.SSLContext:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    return context


def run_case(port: int, steps: list[bytes], tls_mode: str | None = None) -> bytes:
    """Drive one scripted client; returns the application-level transcript."""

    raw = connect(port)
    raw.settimeout(READ_IDLE)
    transcript = b""

    def pump(idle: float = READ_IDLE) -> None:
        nonlocal transcript
        transcript += read_until_idle(raw, idle)

    connection = raw
    if tls_mode == "implicit":
        connection = tls_context().wrap_socket(raw, server_hostname="mail.example.com")
        connection.settimeout(READ_IDLE)

    def pump_active(idle: float = READ_IDLE) -> None:
        nonlocal transcript
        connection.settimeout(idle)
        while True:
            try:
                data = connection.recv(65536)
            except socket.timeout:
                return
            except OSError:
                return
            if not data:
                return
            transcript += data

    pump_active()
    for index, step in enumerate(steps):
        connection.sendall(step)
        if tls_mode == "starttls" and index == 0:
            # STARTTLS flow: greeting + EHLO reply, then the STARTTLS exchange.
            pass
        if tls_mode == "starttls" and step == b"STARTTLS\r\n":
            pump_active()
            connection = tls_context().wrap_socket(
                connection, server_hostname="mail.example.com"
            )
            connection.settimeout(READ_IDLE)
            pump_active()
            continue
        pump_active()
    try:
        connection.close()
    except OSError:
        pass
    return transcript


def normalize_transcript(transcript: bytes) -> str:
    return transcript.decode("latin-1")


def normalize_auth_requests(requests: list[list[str]]) -> list[list[str]]:
    normalized = []
    for headers in requests:
        cleaned = []
        for header in headers:
            if header.lower().startswith("auth-ssl-cipher:"):
                cleaned.append("Auth-SSL-Cipher: <negotiated-suite>")
            else:
                cleaned.append(header)
        normalized.append(cleaned)
    return normalized


CASES = [
    {
        "name": "pop3.login.relay",
        "port": "pop3",
        "steps": [b"USER alice\r\n", b"PASS secret\r\n", b"NOOP\r\n", b"QUIT\r\n"],
    },
    {
        "name": "pop3.auth-login.dialog",
        "port": "pop3",
        "steps": [
            b"AUTH LOGIN\r\n",
            base64.b64encode(b"alice") + b"\r\n",
            base64.b64encode(b"secret") + b"\r\n",
            b"NOOP\r\n",
        ],
    },
    {
        "name": "pop3.auth-failure",
        "port": "pop3",
        "steps": [b"USER bad\r\n", b"PASS secret\r\n"],
    },
    {
        "name": "imap.login.relay",
        "port": "imap",
        "steps": [b"a1 LOGIN alice secret\r\n", b"a2 NOOP\r\n", b"a3 LOGOUT\r\n"],
    },
    {
        "name": "imap.auth-failure",
        "port": "imap",
        "steps": [b"a1 LOGIN bad secret\r\n"],
    },
    {
        "name": "smtp.auth-xclient.relay",
        "port": "smtp",
        "steps": [
            b"EHLO client.example\r\n",
            b"AUTH PLAIN " + LOGIN_PLAIN.encode() + b"\r\n",
            b"QUIT\r\n",
        ],
    },
    {
        "name": "smtp.auth-failure",
        "port": "smtp",
        "steps": [
            b"EHLO client.example\r\n",
            b"AUTH PLAIN " + LOGIN_PLAIN_BAD.encode() + b"\r\n",
        ],
    },
    {
        "name": "smtp.implicit-tls",
        "port": "smtp_tls",
        "tls_mode": "implicit",
        "steps": [
            b"EHLO client.example\r\n",
            b"AUTH PLAIN " + LOGIN_PLAIN.encode() + b"\r\n",
            b"QUIT\r\n",
        ],
    },
    {
        "name": "smtp.starttls",
        "port": "smtp_starttls",
        "tls_mode": "starttls",
        "steps": [
            b"EHLO client.example\r\n",
            b"STARTTLS\r\n",
            b"AUTH PLAIN " + LOGIN_PLAIN.encode() + b"\r\n",
            b"QUIT\r\n",
        ],
    },
]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pop3", type=int, required=True)
    parser.add_argument("--imap", type=int, required=True)
    parser.add_argument("--smtp", type=int, required=True)
    parser.add_argument("--smtp-tls", type=int, required=True)
    parser.add_argument("--smtp-starttls", type=int, required=True)
    parser.add_argument("--auth", type=int, required=True)
    parser.add_argument("--backend-pop3", type=int, required=True)
    parser.add_argument("--backend-imap", type=int, required=True)
    parser.add_argument("--backend-smtp", type=int, required=True)
    parser.add_argument("--server-identity", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    recorder = Recorder()
    backend_ports = {
        "pop3": args.backend_pop3,
        "imap": args.backend_imap,
        "smtp": args.backend_smtp,
    }
    listeners = start_backends(backend_ports, recorder)
    listeners.append(start_auth_http(args.auth, backend_ports, recorder))
    proxies = {
        "pop3": args.pop3,
        "imap": args.imap,
        "smtp": args.smtp,
        "smtp_tls": args.smtp_tls,
        "smtp_starttls": args.smtp_starttls,
    }
    time.sleep(0.2)

    document: dict = {"_server": args.server_identity}
    try:
        for case in CASES:
            recorder.clear_backend_sessions()
            with recorder.lock:
                auth_start = len(recorder.auth_requests)
            transcript = run_case(proxies[case["port"]], case["steps"], case.get("tls_mode"))
            time.sleep(0.1)
            snapshot = recorder.snapshot()
            document[case["name"]] = {
                "client": normalize_transcript(transcript),
                "auth_requests": normalize_auth_requests(
                    snapshot["auth_requests"][auth_start:]
                ),
                "backend_sessions": {
                    protocol: sessions
                    for protocol, sessions in snapshot["backend_sessions"].items()
                    if sessions
                },
            }
    finally:
        for listener in listeners:
            listener.close()
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print("wrote %s (%d cases)" % (args.out, len(CASES)))
    print("identity: %r" % (args.server_identity,))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
