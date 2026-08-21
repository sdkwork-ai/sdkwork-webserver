import argparse
import socket
import time


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=19877)
    parser.add_argument("--count", type=int, default=64)
    args = parser.parse_args()

    for _ in range(100):
        try:
            sock = socket.create_connection(("127.0.0.1", args.port), timeout=1)
            break
        except OSError:
            time.sleep(0.05)
    else:
        raise RuntimeError("Nginx did not become ready")

    request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"
    payload = request * args.count
    with sock:
        sock.settimeout(5)
        sock.sendall(payload)
        data = bytearray()
        while data.count(b"HTTP/1.1 200") < args.count:
            chunk = sock.recv(65536)
            if not chunk:
                break
            data.extend(chunk)
            if len(data) > 1_048_576:
                raise RuntimeError("response exceeded test bound")

    status_count = data.count(b"HTTP/1.1 200")
    print(
        f"requested={args.count}; "
        f"status_200_count={status_count}; "
        f"bytes={len(data)}; "
        f"complete={status_count == args.count}"
    )
    if status_count != args.count:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
