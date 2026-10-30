"""Deterministic uWSGI responder for the differential-protocols slice."""

from __future__ import annotations

import argparse
import json
import os
import socket
import ssl
import struct
import threading
import time


_CACHE_GENERATION = 0
_CACHE_GENERATION_LOCK = threading.Lock()


def cache_response(by_name: dict):
    """Wave 7C: cacheable `gen=<n>` response for the uWSGI cache cases."""
    global _CACHE_GENERATION
    with _CACHE_GENERATION_LOCK:
        _CACHE_GENERATION += 1
        generation = _CACHE_GENERATION
    body = f"gen={generation}".encode()
    return body


def peer_address(connection: socket.socket) -> str:
    peer = connection.getpeername()
    if isinstance(peer, str):
        return "unix"
    return peer[0]

def read_exact(stream: socket.socket, length: int) -> bytes:
    data = b""
    while len(data) < length:
        chunk = stream.recv(length - len(data))
        if not chunk:
            raise EOFError("short uWSGI read")
        data += chunk
    return data


def handle(connection: socket.socket) -> None:
    try:
        header = read_exact(connection, 4)
        modifier1, data_size, modifier2 = struct.unpack("<BHB", header)
        payload = read_exact(connection, data_size)
        with open("/tmp/uwsgi-mock-raw.log", "ab") as handle:
            handle.write(f"header={header.hex()} size={data_size} payload={payload.hex()}\n".encode())
        variables: list[list[str]] = []
        offset = 0
        # nginx appends `uwsgi_string` bytes after the last variable inside the
        # packet; anything that does not parse as a complete variable is the
        # trailing extra string.
        while offset + 4 <= len(payload):
            key_size = struct.unpack("!H", payload[offset : offset + 2])[0]
            if offset + 2 + key_size + 2 > len(payload):
                break
            offset += 2
            key = payload[offset : offset + key_size]
            offset += key_size
            value_size = struct.unpack("!H", payload[offset : offset + 2])[0]
            if offset + 2 + value_size > len(payload):
                break
            offset += 2
            value = payload[offset : offset + value_size]
            offset += value_size
            variables.append([key.decode("utf-8", "replace"), value.decode("utf-8", "replace")])
        extra = payload[offset:]
        by_name = dict(variables)
        body = b""
        try:
            content_length = int(by_name.get("CONTENT_LENGTH") or "0")
        except ValueError:
            content_length = 0
        if content_length > 0:
            body = read_exact(connection, content_length)
        request_uri = by_name.get("REQUEST_URI", "")
        if request_uri.startswith("/cache-uwsgi/"):
            payload = cache_response(by_name)
            head = (
                "HTTP/1.1 200 OK\r\n"
                "Content-Type: text/plain\r\n"
                "X-Mock-Uwsgi: yes\r\n"
                f"Content-Length: {len(payload)}\r\n"
                "\r\n"
            ).encode()
            method = by_name.get("REQUEST_METHOD", "GET")
            connection.sendall(head + (payload if method != "HEAD" else b""))
            return
        status = 404 if "errors" in request_uri else 200
        reason = "Not Found" if status == 404 else "OK"
        response = json.dumps(
            {
                "protocol": "uwsgi",
                "modifier1": modifier1,
                "modifier2": modifier2,
                "extra_hex": extra.hex(),
                "params": sorted(variables),
                "peer": peer_address(connection),
                "body_hex": body.hex(),
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
        head = (
            f"HTTP/1.1 {status} {reason}\r\n"
            "Content-Type: application/json\r\n"
            "X-Mock-Uwsgi: yes\r\n"
            "X-Mock-Hide: hidden\r\n"
            "X-Mock-Pass: passed\r\n"
            f"Content-Length: {len(response)}\r\n"
            "\r\n"
        ).encode()
        method = by_name.get("REQUEST_METHOD", "GET")
        connection.sendall(head + (response if method != "HEAD" else b""))
    except EOFError:
        pass
    finally:
        connection.close()


def serve(listener: socket.socket, tls_context: ssl.SSLContext | None = None) -> None:
    listener.listen(32)
    while True:
        connection, _ = listener.accept()
        if tls_context is not None:
            try:
                connection = tls_context.wrap_socket(connection, server_side=True)
            except (ssl.SSLError, OSError):
                connection.close()
                continue
        threading.Thread(target=handle, args=(connection,), daemon=True).start()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int)
    parser.add_argument("--unix-socket")
    parser.add_argument("--tls-port", type=int, action="append", default=[])
    parser.add_argument("--cert")
    parser.add_argument("--key")
    parser.add_argument("--client-ca")
    arguments = parser.parse_args()
    if arguments.tls_port and not (arguments.cert and arguments.key):
        raise SystemExit("--tls-port requires --cert and --key")
    tls_context = None
    if arguments.tls_port:
        tls_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls_context.load_cert_chain(arguments.cert, arguments.key)
        if arguments.client_ca:
            # Optional client authentication: the mTLS differential case sends
            # the CA-signed client identity, the plain TLS cases send none.
            tls_context.verify_mode = ssl.CERT_OPTIONAL
            tls_context.load_verify_locations(arguments.client_ca)
    for tls_port in arguments.tls_port:
        tls_listener = socket.socket()
        tls_listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        tls_listener.bind(("127.0.0.1", tls_port))
        threading.Thread(
            target=serve, args=(tls_listener, tls_context), daemon=True
        ).start()
    if arguments.unix_socket:
        try:
            os.unlink(arguments.unix_socket)
        except FileNotFoundError:
            pass
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(arguments.unix_socket)
        serve(listener)
    else:
        if arguments.port is None:
            if arguments.tls_port:
                threading.Event().wait()
                return
            raise SystemExit("--port or --unix-socket is required")
        listener = socket.socket()
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", arguments.port))
        serve(listener)


if __name__ == "__main__":
    main()
