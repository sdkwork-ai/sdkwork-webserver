"""Deterministic memcached text-protocol responder for the differential slice."""

from __future__ import annotations

import argparse
import os
import socket
import threading



def peer_address(connection: socket.socket) -> str:
    peer = connection.getpeername()
    if isinstance(peer, str):
        return "unix"
    return peer[0]

def handle(connection: socket.socket) -> None:
    try:
        request = b""
        while not request.endswith(b"\r\n"):
            chunk = connection.recv(1)
            if not chunk:
                return
            request += chunk
        parts = request.decode("latin-1").strip().split(" ")
        if len(parts) != 2 or parts[0] != "get":
            connection.sendall(b"ERROR\r\n")
            return
        key = parts[1]
        if "missing" in key:
            connection.sendall(b"END\r\n")
            return
        if "error" in key:
            connection.sendall(b"ERROR\r\n")
            return
        if "large" in key:
            data = (b"cached-large-" * 683)[:8192]
        else:
            data = f"cached-value:{key}:peer={peer_address(connection)}".encode()
        connection.sendall(
            b"VALUE " + key.encode() + b" 0 " + str(len(data)).encode() + b"\r\n"
            + data
            + b"\r\nEND\r\n"
        )
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int)
    parser.add_argument("--unix-socket")
    arguments = parser.parse_args()
    if arguments.unix_socket:
        try:
            os.unlink(arguments.unix_socket)
        except FileNotFoundError:
            pass
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(arguments.unix_socket)
    else:
        if arguments.port is None:
            raise SystemExit("--port or --unix-socket is required")
        listener = socket.socket()
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", arguments.port))
    listener.listen(32)
    while True:
        connection, _ = listener.accept()
        threading.Thread(target=handle, args=(connection,), daemon=True).start()


if __name__ == "__main__":
    main()
