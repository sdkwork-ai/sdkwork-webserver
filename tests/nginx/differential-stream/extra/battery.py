#!/usr/bin/env python3
"""Drive the stream `split_clients` / `pass` fixture against one server.

The fixture binds the same split-clients listener on six loopback aliases, so
the split source `$server_addr` yields six distinct, port-independent inputs:
the bucket assignment must match nginx's MurmurHash2 thresholds exactly. `pass`
is observable as the target server's own content handler answering on the
passing listener (a `return` value and a proxied echo).

Outcomes use the same normalization as the main stream battery: one
`{"received": "<text>"}` record per connection, and `{"sent", "received"}` for
the echoed proxy case.
"""

from __future__ import annotations

import argparse
import json
import socket
import threading
import time

BUFFER = 4096
SPLIT_ADDRESSES = ["127.0.0.1", "127.0.0.2", "127.0.0.3", "127.0.0.4", "127.0.0.5", "127.0.0.6"]


class EchoUpstream(threading.Thread):
    """Deterministic TCP echo server standing in for the proxied upstream."""

    def __init__(self, port: int) -> None:
        super().__init__(daemon=True)
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.socket.bind(("127.0.0.1", port))
        self.socket.listen(16)
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


def connect(address: str, port: int, deadline_seconds: float = 5.0) -> socket.socket:
    deadline = time.monotonic() + deadline_seconds
    while True:
        try:
            return socket.create_connection((address, port), timeout=4)
        except OSError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)


def read_all(address: str, port: int, timeout: float = 4.0) -> dict:
    try:
        connection = connect(address, port)
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


def exchange(address: str, port: int, payload: bytes, timeout: float = 4.0) -> dict:
    try:
        connection = connect(address, port)
    except OSError as error:
        return {"error": type(error).__name__}
    try:
        connection.settimeout(timeout)
        connection.sendall(payload)
        received = b""
        while True:
            try:
                chunk = connection.recv(BUFFER)
            except (ConnectionResetError, BrokenPipeError, socket.timeout):
                break
            if not chunk:
                break
            received += chunk
            if received.endswith(payload):
                break
        outcome = {"sent": payload.decode()}
        if received:
            outcome["received"] = received.decode()
        return outcome
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--split-port", type=int, required=True)
    parser.add_argument("--pass-target-port", type=int, required=True)
    parser.add_argument("--pass-port", type=int, required=True)
    parser.add_argument("--pass-proxy-target-port", type=int, required=True)
    parser.add_argument("--pass-proxy-port", type=int, required=True)
    parser.add_argument("--upstream-port", type=int, required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    upstream = EchoUpstream(args.upstream_port)
    upstream.start()

    cases: dict[str, list[dict]] = {}
    for index, address in enumerate(SPLIT_ADDRESSES, start=1):
        cases[f"split.server-addr.{index}"] = [read_all(address, args.split_port)]
    cases["pass.return"] = [read_all("127.0.0.1", args.pass_port)]
    cases["pass.proxy"] = [
        exchange("127.0.0.1", args.pass_proxy_port, b"pass-through-echo\n")
    ]
    # The pass target is also directly reachable: the same content handler must
    # answer on the target listener itself (proves `pass` re-dispatches rather
    # than merely aliasing the passing server).
    cases["pass.target-return"] = [read_all("127.0.0.1", args.pass_target_port)]

    upstream.close()
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump({"cases": cases}, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {args.out} ({len(cases)} stream cases)")


if __name__ == "__main__":
    main()
