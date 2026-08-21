import argparse
import socket
import time


CASES = [
    "/a/b",
    "/a//b",
    "/a/./b",
    "/a/../b",
    "/a%2Fb",
    "/a%2fb",
    "/trailing/",
    "/%2e%2e/escape",
]


def receive_response(sock: socket.socket) -> bytes:
    data = bytearray()
    while b"\r\n\r\n" not in data:
        chunk = sock.recv(1)
        if not chunk:
            raise EOFError("connection closed before headers")
        data.extend(chunk)
        if len(data) > 8192:
            raise RuntimeError("response headers exceeded test bound")
    header_end = data.index(b"\r\n\r\n") + 4
    headers = bytes(data[:header_end])
    content_length = 0
    for line in headers.decode("ascii").split("\r\n"):
        name, separator, value = line.partition(":")
        if separator and name.lower() == "content-length":
            content_length = int(value.strip())
    body = bytearray(data[header_end:])
    while len(body) < content_length:
        chunk = sock.recv(content_length - len(body))
        if not chunk:
            raise EOFError("connection closed before body")
        body.extend(chunk)
    return bytes(body)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=19876)
    args = parser.parse_args()

    for _ in range(100):
        try:
            sock = socket.create_connection(("127.0.0.1", args.port), timeout=1)
            sock.close()
            break
        except OSError:
            time.sleep(0.05)
    else:
        raise RuntimeError("Nginx did not become ready")

    for path in CASES:
        with socket.create_connection(("127.0.0.1", args.port), timeout=2) as sock:
            sock.settimeout(2)
            sock.sendall(
                f"GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".encode(
                    "ascii"
                )
            )
            body = receive_response(sock).decode("utf-8").rstrip("\n")
            print(f"{path} => {body}")


if __name__ == "__main__":
    main()
