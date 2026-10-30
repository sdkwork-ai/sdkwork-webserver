#!/usr/bin/env python3
"""Drive identical connection scripts against a stream proxy and record outcomes.

The stream plane has no HTTP framing to compare; the observable contract is what
each connection receives. An outcome is one of:

  {"sent": "<text>", "received": "<text>"}   the bytes were echoed end to end
  {"sent": "<text>"}                          the connection was closed by the
                                              server before any bytes came back
  {"closed": true}                            closed before the client wrote
                                              anything (access deny)
  {"error": "<connect failure>"}              the listener refused the connection

"Closed by the server" is normalized: depending on whether the client's bytes
reach the dying socket before the FIN, the peer observes EOF or ECONNRESET; both
mean the same nginx contract (silent close without proxying) and the race is not
a behavioural difference.
"""

from __future__ import annotations

import argparse
import json
import socket
import ssl
import threading
import time

HOST = "127.0.0.1"
BUFFER = 4096


class EchoUpstream(threading.Thread):
    """Deterministic TCP echo server standing in for the proxied upstream."""

    def __init__(self, port: int) -> None:
        super().__init__(daemon=True)
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.socket.bind((HOST, port))
        self.socket.listen(32)
        self.port = self.socket.getsockname()[1]
        self.stopped = threading.Event()

    def run(self) -> None:
        while not self.stopped.is_set():
            try:
                connection, _ = self.socket.accept()
            except OSError:
                return
            threading.Thread(target=self._echo, args=(connection,), daemon=True).start()

    def _echo(self, connection: socket.socket) -> None:
        try:
            while True:
                data = connection.recv(BUFFER)
                if not data:
                    return
                connection.sendall(data)
        except OSError:
            return
        finally:
            connection.close()

    def close(self) -> None:
        self.stopped.set()
        try:
            self.socket.close()
        except OSError:
            pass


def connect(port: int, deadline_seconds: float = 5.0) -> socket.socket:
    deadline = time.monotonic() + deadline_seconds
    while True:
        try:
            return socket.create_connection((HOST, port), timeout=4)
        except OSError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)


def exchange(port: int, payload: bytes, timeout: float = 4.0) -> dict:
    """Write one message and record whether the server echoes it back."""
    try:
        connection = connect(port)
    except OSError as error:
        return {"error": type(error).__name__}
    try:
        connection.settimeout(timeout)
        connection.sendall(payload)
        try:
            received = connection.recv(BUFFER)
        except (ConnectionResetError, BrokenPipeError, socket.timeout):
            return {"sent": payload.decode()}
        if not received:
            return {"sent": payload.decode()}
        return {"sent": payload.decode(), "received": received.decode()}
    finally:
        connection.close()


def quiet_close(port: int, timeout: float = 4.0) -> dict:
    """Connect without writing and record whether the server closes silently."""
    try:
        connection = connect(port)
    except OSError as error:
        return {"error": type(error).__name__}
    try:
        connection.settimeout(timeout)
        try:
            received = connection.recv(BUFFER)
        except (ConnectionResetError, BrokenPipeError):
            return {"closed": True}
        except socket.timeout:
            return {"timed_out": True}
        if not received:
            return {"closed": True}
        return {"received": received.decode()}
    finally:
        connection.close()


def wait_for_echo(port: int, payload: bytes, deadline_seconds: float = 5.0) -> dict:
    """Retry the echo exchange until the released slot is observable.

    `limit_conn` releases a slot when the previous connection's task finishes,
    which is a scheduling moment; nginx releases it when the connection closes.
    Retrying the *first* echo after a close compares the released-limit
    contract, not the microsecond a task tears down.
    """
    deadline = time.monotonic() + deadline_seconds
    last = exchange(port, payload)
    while "received" not in last and time.monotonic() < deadline:
        time.sleep(0.05)
        last = exchange(port, payload)
    return last


def read_all(port: int, timeout: float = 4.0) -> dict:
    """Connect, read until the server closes, and record the received bytes."""
    try:
        connection = connect(port)
    except OSError as error:
        return {"error": type(error).__name__}
    try:
        connection.settimeout(timeout)
        received = b""
        while True:
            try:
                chunk = connection.recv(BUFFER)
            except (ConnectionResetError, socket.timeout):
                break
            if not chunk:
                break
            received += chunk
        return {"received": received.decode()}
    finally:
        connection.close()


def tls_client_hello(server_name: str) -> bytes:
    """Build the ClientHello bytes for one SNI without completing a handshake."""
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    incoming = ssl.MemoryBIO()
    outgoing = ssl.MemoryBIO()
    connection = context.wrap_bio(
        incoming, outgoing, server_side=False, server_hostname=server_name
    )
    try:
        connection.do_handshake()
    except ssl.SSLWantReadError:
        pass
    return outgoing.read()


def preread_map_return(port: int, server_name: str) -> dict:
    """Send a ClientHello, then read the plaintext `return` value."""
    hello = tls_client_hello(server_name)
    try:
        connection = connect(port)
    except OSError as error:
        return {"sni": server_name, "error": type(error).__name__}
    try:
        connection.settimeout(4)
        connection.sendall(hello)
        received = b""
        while True:
            try:
                chunk = connection.recv(BUFFER)
            except (ConnectionResetError, socket.timeout):
                break
            if not chunk:
                break
            received += chunk
        return {"sni": server_name, "received": received.decode()}
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port0", type=int, required=True)
    parser.add_argument("--port1", type=int, required=True)
    parser.add_argument("--port2", type=int, required=True)
    parser.add_argument("--port3", type=int, required=True)
    parser.add_argument("--port4", type=int, required=True)
    parser.add_argument("--port5", type=int, required=True)
    parser.add_argument("--port6", type=int, required=True)
    parser.add_argument("--port7", type=int, required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--upstream-port", type=int, required=True)
    args = parser.parse_args()

    upstream = EchoUpstream(args.upstream_port)
    upstream.start()

    cases: dict[str, list[dict]] = {}

    # Plain proxy path: two sequential round trips on one TCP connection.
    connection = connect(args.port0)
    try:
        connection.settimeout(4)
        outcomes = []
        for payload in (b"stream-echo-one\n", b"stream-echo-two\n"):
            connection.sendall(payload)
            received = b""
            while len(received) < len(payload):
                chunk = connection.recv(BUFFER)
                if not chunk:
                    break
                received += chunk
            outcomes.append(
                {"sent": payload.decode(), "received": received.decode()}
            )
        cases["echo.roundtrip"] = outcomes
    finally:
        connection.close()

    # Access phase: `deny 127.0.0.1; allow all;` closes without proxying.
    cases["access.deny-loopback"] = [quiet_close(args.port1)]

    # `return 403` writes the literal value then closes.
    connection = connect(args.port2)
    try:
        connection.settimeout(4)
        received = b""
        while True:
            chunk = connection.recv(BUFFER)
            if not chunk:
                break
            received += chunk
        cases["return.403"] = [{"received": received.decode()}]
    finally:
        connection.close()

    # limit_conn: the first connection holds the only slot, the second is closed
    # silently, and the slot is released after the holder closes.
    first = connect(args.port3)
    second_outcome = exchange(args.port3, b"over-limit-probe\n")
    first.sendall(b"limit-holder\n")
    holder = first.recv(BUFFER)
    first.close()
    released = wait_for_echo(args.port3, b"after-release\n")
    cases["limit_conn.enforce"] = [
        {"received": holder.decode()},
        second_outcome,
        released,
    ]

    # limit_conn_dry_run: over-limit connections are counted but still proxied.
    dry_first = connect(args.port4)
    dry_second = exchange(args.port4, b"dry-run-second\n")
    dry_first.sendall(b"dry-run-first\n")
    dry_first_received = dry_first.recv(BUFFER)
    dry_first.close()
    cases["limit_conn.dry-run"] = [
        dry_second,
        {"received": dry_first_received.decode()},
    ]

    # Bounded stream variable engine: literal `set` value, `map` result over an
    # accept-time fact, and a `map` over `$ssl_preread_server_name` (read from a
    # synthetic TLS ClientHello) all feed `return <value>`.
    cases["set.return-literal"] = [read_all(args.port5)]

    cases["map.return-remote-addr"] = [read_all(args.port6)]

    cases["preread.map.return-sni"] = [preread_map_return(args.port7, "a.example")]
    cases["preread.map.return-default"] = [preread_map_return(args.port7, "b.example")]

    upstream.close()
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump({"cases": cases}, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {args.out} ({len(cases)} stream cases)")


if __name__ == "__main__":
    main()
