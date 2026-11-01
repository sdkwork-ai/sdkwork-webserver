#!/usr/bin/env python3
"""Mail client-certificate differential battery (real nginx vs Rust plane).

Every case records the client-visible wire bytes and the exact auth_http request
headers. The client certificate identities are committed fixtures, so
`Auth-SSL-Subject`, `Auth-SSL-Issuer`, `Auth-SSL-Serial`, `Auth-SSL-Fingerprint`
and the PEM in `Auth-SSL-Cert` are byte-comparable. `Auth-SSL-Cipher` is
normalized because nginx reports the OpenSSL cipher name while rustls reports
its own suite name (the negotiated suite is proven by the TLS handshake, not by
the label) — the same allowance the main mail battery documents.
"""

from __future__ import annotations

import argparse
import json
import socket
import ssl
import threading
import time

READ_IDLE = 0.45
CONNECT_TIMEOUT = 10.0
LOGIN_PLAIN = "AGFsaWNlAHNlY3JldA=="  # \0alice\0secret


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
    listener.listen(32)

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
    connection.settimeout(10.0)
    pending = b""
    try:
        while b"\r\n\r\n" not in pending:
            data = connection.recv(65536)
            if not data:
                return
            pending += data
        lines = pending.decode("latin-1").split("\r\n")
        recorder.add_auth([line for line in lines if line])
        response = (
            "HTTP/1.0 200 OK\r\n"
            "Auth-Status: OK\r\n"
            "Auth-User: alice\r\n"
            "Auth-Server: 127.0.0.1\r\n"
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
    listener.listen(32)

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
    connection.settimeout(10.0)
    try:
        connection.sendall(b"220 backend.example ESMTP\r\n")
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
    received = b""
    connection.settimeout(idle)
    while True:
        try:
            chunk = connection.recv(65536)
        except socket.timeout:
            break
        except OSError:
            break
        if not chunk:
            break
        received += chunk
    return received


def tls_context(cert: str | None, key: str | None) -> ssl.SSLContext:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    if cert:
        context.load_cert_chain(cert, key)
    return context


def wrap(raw: socket.socket, cert: str | None, key: str | None) -> ssl.SSLSocket:
    return tls_context(cert, key).wrap_socket(raw, server_hostname="mail.example.com")


def send(connection, data: bytes) -> bool:
    try:
        connection.sendall(data)
        return True
    except OSError:
        return False


def smtp_auth_exchange(connection: ssl.SSLSocket) -> bytes:
    transcript = read_until_idle(connection)
    if not send(connection, b"EHLO client.example\r\n"):
        return transcript
    transcript += read_until_idle(connection)
    if not send(connection, f"AUTH PLAIN {LOGIN_PLAIN}\r\n".encode()):
        return transcript
    transcript += read_until_idle(connection)
    return transcript


def case_implicit(port: int, cert: str | None, key: str | None) -> dict:
    raw = connect(port)
    try:
        connection = wrap(raw, cert, key)
        transcript = smtp_auth_exchange(connection)
        connection.close()
        return {"client": transcript.decode("latin-1")}
    except (ssl.SSLError, OSError) as error:
        return {"client": f"TLS-ERROR:{error}"}
    finally:
        raw.close()


def case_starttls(port: int, cert: str | None, key: str | None) -> dict:
    raw = connect(port)
    try:
        transcript = read_until_idle(raw)
        if not send(raw, b"EHLO client.example\r\n"):
            return {"client": transcript.decode("latin-1")}
        transcript += read_until_idle(raw)
        if not send(raw, b"STARTTLS\r\n"):
            return {"client": transcript.decode("latin-1")}
        transcript += read_until_idle(raw)
        connection = wrap(raw, cert, key)
        if send(connection, f"AUTH PLAIN {LOGIN_PLAIN}\r\n".encode()):
            transcript += read_until_idle(connection)
        connection.close()
        return {"client": transcript.decode("latin-1")}
    except ssl.SSLError as error:
        return {"client": (transcript + f"TLS-ERROR:{error}".encode()).decode("latin-1")}
    finally:
        raw.close()


def case_greeting_only(port: int, cert: str | None, key: str | None) -> dict:
    raw = connect(port)
    try:
        connection = wrap(raw, cert, key)
        transcript = read_until_idle(connection)
        connection.close()
        return {"client": transcript.decode("latin-1")}
    except ssl.SSLError as error:
        return {"client": f"TLS-ERROR:{error}"}
    finally:
        raw.close()


def normalize_auth_requests(requests: list[list[str]]) -> list[list[str]]:
    normalized = []
    for headers in requests:
        normalized.append(
            [
                "Auth-SSL-Cipher: <negotiated-suite>"
                if header.lower().startswith("auth-ssl-cipher:")
                else header
                for header in headers
            ]
        )
    return normalized


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--smtp-on", type=int, required=True)
    parser.add_argument("--smtp-on-pass-off", type=int, required=True)
    parser.add_argument("--smtp-trusted-only", type=int, required=True)
    parser.add_argument("--smtp-optional", type=int, required=True)
    parser.add_argument("--smtp-no-ca", type=int, required=True)
    parser.add_argument("--smtp-depth0", type=int, required=True)
    parser.add_argument("--smtp-depth1", type=int, required=True)
    parser.add_argument("--smtp-depth2", type=int, required=True)
    parser.add_argument("--smtp-trusted-intermediate", type=int, required=True)
    parser.add_argument("--smtp-starttls", type=int, required=True)
    parser.add_argument("--imap-on", type=int, required=True)
    parser.add_argument("--pop3-on", type=int, required=True)
    parser.add_argument("--smtp-verify-off", type=int, required=True)
    parser.add_argument("--auth-port", type=int, required=True)
    parser.add_argument("--backend-port", type=int, required=True)
    parser.add_argument("--ca", required=True)
    parser.add_argument("--client-trusted-cert", required=True)
    parser.add_argument("--client-trusted-key", required=True)
    parser.add_argument("--client-untrusted-cert", required=True)
    parser.add_argument("--client-untrusted-key", required=True)
    parser.add_argument("--client-selfsigned-cert", required=True)
    parser.add_argument("--client-selfsigned-key", required=True)
    parser.add_argument("--client-intermediate-cert", required=True)
    parser.add_argument("--client-intermediate-chain", required=True)
    parser.add_argument("--client-intermediate-key", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    recorder = Recorder()
    listeners = [
        serve_auth(args.auth_port, recorder, args.backend_port),
        serve_backend(args.backend_port),
    ]
    time.sleep(0.2)

    trusted = (args.client_trusted_cert, args.client_trusted_key)
    untrusted = (args.client_untrusted_cert, args.client_untrusted_key)
    selfsigned = (args.client_selfsigned_cert, args.client_selfsigned_key)
    intermediate = (args.client_intermediate_chain, args.client_intermediate_key)
    intermediate_leaf = (args.client_intermediate_cert, args.client_intermediate_key)

    def record(name: str, runner, port: int, identity) -> None:
        with recorder.lock:
            start = len(recorder.auth_requests)
        result = runner(port, identity[0] if identity else None, identity[1] if identity else None)
        time.sleep(0.1)
        result["auth_requests"] = normalize_auth_requests(recorder.snapshot()[start:])
        cases[name] = result

    cases: dict[str, object] = {}
    try:
        record("verify-on.trusted", case_implicit, args.smtp_on, trusted)
        record("verify-on.untrusted", case_implicit, args.smtp_on, untrusted)
        record("verify-on.missing", case_implicit, args.smtp_on, None)
        record("verify-on.pass-cert-off", case_implicit, args.smtp_on_pass_off, trusted)
        record("verify-trusted-bundle", case_implicit, args.smtp_trusted_only, trusted)
        record("verify-optional.missing", case_implicit, args.smtp_optional, None)
        record("verify-optional.untrusted", case_implicit, args.smtp_optional, untrusted)
        record("verify-no-ca.untrusted", case_implicit, args.smtp_no_ca, untrusted)
        record("verify-no-ca.self-signed", case_implicit, args.smtp_no_ca, selfsigned)
        record("verify-depth0.direct", case_implicit, args.smtp_depth0, trusted)
        record("verify-depth0.intermediate", case_implicit, args.smtp_depth0, intermediate)
        record("verify-depth1.intermediate", case_implicit, args.smtp_depth1, intermediate)
        record("verify-depth2.intermediate", case_implicit, args.smtp_depth2, intermediate)
        record(
            "verify-trusted-intermediate.leaf-only",
            case_implicit,
            args.smtp_trusted_intermediate,
            intermediate_leaf,
        )
        record("verify-starttls.trusted", case_starttls, args.smtp_starttls, trusted)
        record("verify-starttls.untrusted", case_starttls, args.smtp_starttls, untrusted)
        record("verify-imap.missing", case_greeting_only, args.imap_on, None)
        record("verify-pop3.missing", case_greeting_only, args.pop3_on, None)
        record("verify-off.client-cert-loaded", case_implicit, args.smtp_verify_off, trusted)
    finally:
        for listener in listeners:
            listener.close()

    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {args.out} ({len(cases)} mail client-cert cases)")


if __name__ == "__main__":
    main()
