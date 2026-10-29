"""Deterministic FastCGI 1.0 responder for the differential-protocols slice.

The same responder backs the real-nginx oracle and the Rust data plane, so the
two sides must send the same request the responder can observe: every response
embeds the received `PARAMS` (sorted) and the raw `STDIN` bytes. Status and
headers are driven by the request target so the battery can assert response
mapping (hide/pass, error_page interception, upstream retry) as well.
"""

from __future__ import annotations

import argparse
import json
import socket
import struct
import threading


def read_exact(stream: socket.socket, length: int) -> bytes:
    data = b""
    while len(data) < length:
        chunk = stream.recv(length - len(data))
        if not chunk:
            raise EOFError("short FastCGI record")
        data += chunk
    return data


def read_variable_length(payload: bytes, offset: int) -> tuple[int, int]:
    first = payload[offset]
    offset += 1
    if first < 0x80:
        return first, offset
    length = (
        ((first & 0x7F) << 24)
        | (payload[offset] << 16)
        | (payload[offset + 1] << 8)
        | payload[offset + 2]
    )
    return length, offset + 3


def record(record_type: int, content: bytes, request_id: int = 1) -> bytes:
    padding = (8 - len(content) % 8) % 8
    return (
        struct.pack("!BBHHBB", 1, record_type, request_id, len(content), padding, 0)
        + content
        + b"\x00" * padding
    )


def handle(connection: socket.socket) -> None:
    try:
        params: list[list[str]] = []
        body = b""
        while True:
            header = read_exact(connection, 8)
            version, record_type, request_id, length, padding, _ = struct.unpack(
                "!BBHHBB", header
            )
            if version != 1:
                return
            content = read_exact(connection, length)
            if padding:
                read_exact(connection, padding)
            if record_type == 4:  # FCGI_PARAMS
                offset = 0
                while offset < len(content):
                    name_length, offset = read_variable_length(content, offset)
                    value_length, offset = read_variable_length(content, offset)
                    name = content[offset : offset + name_length]
                    offset += name_length
                    value = content[offset : offset + value_length]
                    offset += value_length
                    params.append(
                        [name.decode("utf-8", "replace"), value.decode("utf-8", "replace")]
                    )
            elif record_type == 5:  # FCGI_STDIN
                body += content
                if length == 0:
                    break
            elif record_type == 2:  # FCGI_ABORT_REQUEST
                return
        by_name = dict(params)
        request_uri = by_name.get("REQUEST_URI", "")
        status = 200
        if request_uri.startswith("/errors/"):
            status = 404
        elif "/status/" in request_uri:
            try:
                status = int(request_uri.split("/status/")[1].split("?")[0])
            except ValueError:
                status = 500
        payload = json.dumps(
            {
                "params": sorted(params),
                "body_hex": body.hex(),
                "script_name": by_name.get("SCRIPT_NAME", ""),
                "path_info": by_name.get("PATH_INFO", ""),
                "request_method": by_name.get("REQUEST_METHOD", ""),
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
        reason = {
            200: "OK",
            404: "Not Found",
            503: "Service Unavailable",
        }.get(status, "Status")
        head = (
            f"Status: {status} {reason}\r\n"
            "Content-Type: application/json\r\n"
            "X-Mock-Fcgi: yes\r\n"
            "X-Accel-Expires: 0\r\n"
            "X-Mock-Hide: hidden\r\n"
            "X-Mock-Pass: passed\r\n"
            f"Content-Length: {len(payload)}\r\n"
            "\r\n"
        ).encode()
        emit_body = by_name.get("REQUEST_METHOD", "GET") != "HEAD"
        connection.sendall(record(7, b"mock-stderr\n"))
        connection.sendall(record(6, head + (payload if emit_body else b"")))
        connection.sendall(record(6, b""))
        connection.sendall(record(3, struct.pack("!IB3x", 0, 0)))
    except EOFError:
        pass
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    arguments = parser.parse_args()
    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", arguments.port))
    listener.listen(32)
    while True:
        connection, _ = listener.accept()
        threading.Thread(target=handle, args=(connection,), daemon=True).start()


if __name__ == "__main__":
    main()
