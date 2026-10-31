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


class TLSEchoUpstream(threading.Thread):
    """TLS-terminating echo upstream (nginx `proxy_ssl on` target)."""

    def __init__(self, port: int, certificate: str, private_key: str) -> None:
        super().__init__(daemon=True)
        self.context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.context.load_cert_chain(certificate, private_key)
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.socket.bind((HOST, port))
        self.socket.listen(32)
        self.stopped = threading.Event()

    def run(self) -> None:
        while not self.stopped.is_set():
            try:
                connection, _ = self.socket.accept()
            except OSError:
                return
            threading.Thread(
                target=self._serve, args=(connection,), daemon=True
            ).start()

    def _serve(self, connection: socket.socket) -> None:
        try:
            tls = self.context.wrap_socket(connection, server_side=True)
        except (ssl.SSLError, OSError):
            connection.close()
            return
        try:
            while True:
                data = tls.recv(BUFFER)
                if not data:
                    return
                tls.sendall(data)
        except OSError:
            return
        finally:
            tls.close()

    def close(self) -> None:
        self.stopped.set()
        try:
            self.socket.close()
        except OSError:
            pass


class HalfCloseEchoUpstream(threading.Thread):
    """Upstream that answers only after the client half-closes its write side."""

    def __init__(self, port: int) -> None:
        super().__init__(daemon=True)
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.socket.bind((HOST, port))
        self.socket.listen(32)
        self.stopped = threading.Event()

    def run(self) -> None:
        while not self.stopped.is_set():
            try:
                connection, _ = self.socket.accept()
            except OSError:
                return
            threading.Thread(
                target=self._serve, args=(connection,), daemon=True
            ).start()

    def _serve(self, connection: socket.socket) -> None:
        try:
            received = b""
            while True:
                data = connection.recv(BUFFER)
                if not data:
                    break
                received += data
            connection.sendall(f"got:{len(received)}".encode())
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


def exchange_with_prefix(port: int, prefix: bytes, payload: bytes) -> dict:
    """Write a prefix (an inbound PROXY header) plus one payload, record echo."""
    try:
        connection = connect(port)
    except OSError as error:
        return {"error": type(error).__name__}
    try:
        connection.settimeout(4)
        try:
            connection.sendall(prefix + payload)
        except OSError:
            return {"sent": payload.decode()}
        received = b""
        while True:
            try:
                chunk = connection.recv(BUFFER)
            except (ConnectionResetError, BrokenPipeError):
                break
            if not chunk:
                break
            received += chunk
            if received.endswith(payload):
                break
        outcome = {"sent": payload.decode()}
        if received:
            outcome["received"] = received.decode(errors="replace")
        return outcome
    finally:
        connection.close()


def half_close_exchange(port: int, payload: bytes) -> dict:
    """Send a payload, half-close the write side, then record the reply."""
    try:
        connection = connect(port)
    except OSError as error:
        return {"error": type(error).__name__}
    try:
        connection.settimeout(5)
        try:
            connection.sendall(payload)
            connection.shutdown(socket.SHUT_WR)
        except OSError:
            return {"sent": payload.decode()}
        received = b""
        while True:
            try:
                chunk = connection.recv(BUFFER)
            except (ConnectionResetError, BrokenPipeError):
                break
            if not chunk:
                break
            received += chunk
        outcome = {"sent": payload.decode()}
        if received:
            outcome["received"] = received.decode(errors="replace")
        return outcome
    finally:
        connection.close()


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
    parser.add_argument("--port8", type=int, required=True)
    parser.add_argument("--port9", type=int, required=True)
    parser.add_argument("--port10", type=int, required=True)
    parser.add_argument("--port11", type=int, required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--upstream-port", type=int, required=True)
    parser.add_argument("--tls-echo-port", type=int, required=True)
    parser.add_argument("--tls-echo-2-port", type=int, required=True)
    parser.add_argument("--half-close-echo-port", type=int, required=True)
    parser.add_argument("--tls-cert", required=True)
    parser.add_argument("--tls-key", required=True)
    args = parser.parse_args()

    upstream = EchoUpstream(args.upstream_port)
    upstream.start()
    tls_upstream = TLSEchoUpstream(args.tls_echo_port, args.tls_cert, args.tls_key)
    tls_upstream.start()
    tls_upstream_2 = TLSEchoUpstream(args.tls_echo_2_port, args.tls_cert, args.tls_key)
    tls_upstream_2.start()
    half_close_upstream = HalfCloseEchoUpstream(args.half_close_echo_port)
    half_close_upstream.start()

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

    # Wave 7E: inbound PROXY protocol (`listen … proxy_protocol`). The header
    # source is consumed (and trusted for realip in the typed configuration),
    # then the payload is proxied byte-identically.
    cases["proxy_protocol.inbound-v1"] = [
        exchange_with_prefix(
            args.port8,
            b"PROXY TCP4 10.0.0.5 127.0.0.1 1111 80\r\n",
            b"proxy-in-echo\n",
        )
    ]
    # A listener that requires a PROXY header rejects a connection without one.
    cases["proxy_protocol.missing-header"] = [
        exchange_with_prefix(args.port8, b"garbage-not-a-proxy", b"payload\n")
    ]

    # Wave 7E: upstream TLS client. `proxy_ssl on` with a verified CA bundle and
    # the `proxy_ssl_name` SNI override carries plaintext through the TLS tunnel
    # to the echo upstream.
    cases["proxy_ssl.verified"] = [
        exchange_with_prefix(args.port9, b"", b"tls-echo-payload\n")
    ]
    # An unrelated CA bundle fails the upstream handshake; the connection closes
    # without proxying (status-only allowance: nginx may log/close differently).
    cases["proxy_ssl.untrusted-ca"] = [
        exchange_with_prefix(args.port10, b"", b"must-not-arrive\n")
    ]

    # Wave 7E: `proxy_half_close on` forwards the client half-close and keeps
    # the reverse direction alive until the upstream answers.
    cases["half_close.on"] = [half_close_exchange(args.port11, b"half-close-payload")]

    upstream.close()
    tls_upstream.close()
    tls_upstream_2.close()
    half_close_upstream.close()
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump({"cases": cases}, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {args.out} ({len(cases)} stream cases)")


if __name__ == "__main__":
    main()
