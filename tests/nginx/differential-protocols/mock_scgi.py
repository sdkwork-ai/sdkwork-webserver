"""Deterministic SCGI responder for the differential-protocols slice."""

from __future__ import annotations

import argparse
import json
import socket
import threading


def read_exact(stream: socket.socket, length: int) -> bytes:
    data = b""
    while len(data) < length:
        chunk = stream.recv(length - len(data))
        if not chunk:
            raise EOFError("short SCGI read")
        data += chunk
    return data


def handle(connection: socket.socket) -> None:
    try:
        header_length = b""
        while not header_length.endswith(b":"):
            header_length += read_exact(connection, 1)
            if len(header_length) > 12:
                raise EOFError("SCGI netstring length too long")
        length = int(header_length[:-1])
        content = read_exact(connection, length + 1)  # trailing comma
        if content[-1:] != b",":
            raise EOFError("missing SCGI netstring terminator")
        fields = content[:-1].split(b"\x00")
        variables: list[list[str]] = []
        for index in range(0, len(fields) - 1, 2):
            variables.append(
                [
                    fields[index].decode("utf-8", "replace"),
                    fields[index + 1].decode("utf-8", "replace"),
                ]
            )
        by_name = dict(variables)
        try:
            content_length = int(by_name.get("CONTENT_LENGTH") or "0")
        except ValueError:
            content_length = 0
        body = read_exact(connection, content_length) if content_length > 0 else b""
        request_uri = by_name.get("REQUEST_URI", "")
        status = 404 if "errors" in request_uri else 200
        reason = "Not Found" if status == 404 else "OK"
        response = json.dumps(
            {
                "protocol": "scgi",
                "params": sorted(variables),
                "body_hex": body.hex(),
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
        head = (
            f"HTTP/1.1 {status} {reason}\r\n"
            "Content-Type: application/json\r\n"
            "X-Mock-Scgi: yes\r\n"
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
