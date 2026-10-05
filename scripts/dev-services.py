#!/usr/bin/env python3
"""Start the dev Compose services on free host ports and configure the backend."""

import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
from urllib.error import URLError
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[1]
DEV_DIR = ROOT / ".dev"
PORTS_FILE = DEV_DIR / "ports.json"

# name: (container name, container port, preferred host port)
SERVICES = {
    "POSTGRES": ("axismundi-db", "5432/tcp", 5432),
    "GARAGE": ("axismundi-garage", "3900/tcp", 9000),
    "GARAGE_ADMIN": ("axismundi-garage", "3903/tcp", 9003),
    "IMAGOR": ("axismundi-imagor", "8000/tcp", 8888),
    "LEXURGY": ("axismundi-lexurgy", "8080/tcp", 8080),
    "APP": (None, None, 3000),
}


def published_port(container, container_port):
    result = subprocess.run(
        ["docker", "inspect", container], cwd=ROOT, capture_output=True, text=True
    )
    if result.returncode:
        return None
    info = json.loads(result.stdout)[0]
    if not info["State"]["Running"]:
        return None
    bindings = info["NetworkSettings"]["Ports"].get(container_port) or []
    return int(bindings[0]["HostPort"]) if bindings else None


def port_is_free(port):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        try:
            sock.bind(("0.0.0.0", port))
        except OSError:
            return False
    return True


def port_processes(port):
    try:
        result = subprocess.run(
            ["ss", "-H", "-ltnp", f"sport = :{port}"],
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return []
    if result.returncode:
        return []
    return re.findall(r'\("([^"]+)",pid=(\d+)', result.stdout)


def app_owns_port(port):
    for _, pid in port_processes(port):
        try:
            executable = Path(f"/proc/{pid}/exe").resolve(strict=True)
            directory = Path(f"/proc/{pid}/cwd").resolve(strict=True)
        except OSError:
            continue
        if (
            directory == ROOT
            and executable.name == "axismundi"
            and executable.is_relative_to(ROOT / "target")
        ):
            return True
    return False


def port_owner(port):
    processes = port_processes(port)
    if not processes:
        return None
    process, pid = processes[0]
    process = process.split(" (")[0]
    if process == "rootlessport":
        containers = subprocess.run(
            ["docker", "ps", "--format", "{{.Names}} {{.Ports}}"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        for line in containers.stdout.splitlines():
            if re.search(rf":{port}-", line):
                return f"container {line.split()[0]}"
    try:
        directory = Path(f"/proc/{pid}/cwd").resolve()
    except OSError:
        directory = None
    location = f" in {directory}" if directory and "projects" in directory.parts and directory != ROOT else ""
    return f"{process} (PID {pid}){location}"


def select_ports():
    saved = json.loads(PORTS_FILE.read_text()) if PORTS_FILE.exists() else {}
    selected = {}
    used = set()
    for name, (container, container_port, preferred) in SERVICES.items():
        running = published_port(container, container_port) if container else None
        # 3001 is reserved for the test app in src/config.rs.
        fallback_start = 3002 if name == "APP" else preferred + 1
        candidates = [running, saved.get(name), preferred, *range(fallback_start, 65536)]
        for port in candidates:
            if port is None or port in used:
                continue
            if (
                port == running
                or port_is_free(port)
                or (name == "APP" and app_owns_port(port))
            ):
                selected[name] = port
                used.add(port)
                if port != preferred:
                    if port_is_free(preferred):
                        print(f"{name.lower()}: reusing port {port}", flush=True)
                    else:
                        owner = port_owner(preferred)
                        detail = f" ({owner})" if owner else ""
                        print(
                            f"{name.lower()}: {preferred} is busy{detail}; using {port}",
                            flush=True,
                        )
                break
        else:
            raise RuntimeError(f"No free host port found for {name.lower()}")
    return selected


def write_json(path, data):
    DEV_DIR.mkdir(exist_ok=True)
    temporary = path.with_suffix(".tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as output:
        json.dump(data, output, indent=2)
        output.write("\n")
    os.replace(temporary, path)


def write_config(ports):
    local = ROOT / "config.json"
    example = json.loads((ROOT / "resources/config.json").read_text())
    config = json.loads(local.read_text()) if local.exists() else example
    config["database_url"] = (
        f"postgres://user:password@localhost:{ports['POSTGRES']}/axismundi"
    )
    config["s3"]["bucket"] = example["s3"]["bucket"]
    config["s3"]["region"] = example["s3"]["region"]
    config["s3"]["access_key"] = example["s3"]["access_key"]
    config["s3"]["secret_key"] = example["s3"]["secret_key"]
    config["s3"]["endpoint"] = f"http://localhost:{ports['GARAGE']}"
    config["s3"]["public_url_base"] = f"http://localhost:{ports['IMAGOR']}"
    config["s3"]["imagor_secret"] = "change-me-in-production"
    config["lexurgy"] = {
        "url": f"http://localhost:{ports['LEXURGY']}",
        "api_key": "change-me-in-production",
    }
    config["port"] = ports["APP"]
    config["public_url_base"] = f"http://localhost:{ports['APP']}"
    config.setdefault("maid", {
        "port": 3003,
        "health_check_timeout_ms": 10000,
        "wait_between_tasks_ms": 1000,
        "task_timeout_ms": 15000,
    })
    write_json(DEV_DIR / "config.json", config)
    write_json(PORTS_FILE, ports)


def wait_for(label, ready):
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        if ready():
            return
        time.sleep(1)
    raise RuntimeError(f"{label} did not become ready within 120 seconds")


def http_ready(url):
    try:
        with urlopen(url, timeout=2) as response:
            return response.status < 400
    except (OSError, URLError):
        return False


def main():
    if sys.argv[1:] in (["url"], ["database-url"]):
        path = DEV_DIR / "config.json"
        if not path.exists():
            path = ROOT / "config.json"
        field = "database_url" if sys.argv[1] == "database-url" else "public_url_base"
        value = json.loads(path.read_text())[field]
        print(value if field == "database_url" else value.rstrip("/"))
        return
    if sys.argv[1:] != ["up"]:
        raise SystemExit("usage: scripts/dev-services.py up|url|database-url")
    ports = select_ports()
    environment = os.environ.copy()
    environment.update({f"DEV_{name}_PORT": str(port) for name, port in ports.items()})
    print("Starting all development services...", flush=True)
    subprocess.run(
        ["docker", "compose", "up", "--no-deps", "-d", "postgres", "garage", "imagor", "lexurgy"],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    # The bucket seeder exits after copying. Podman Compose's normal `up`
    # waits forever for an already-exited dependency to become "running".
    subprocess.run(
        ["docker", "compose", "run", "--rm", "--no-deps", "-T", "seedbucket"],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    write_config(ports)
    print("Waiting for services to be ready...", flush=True)
    wait_for("PostgreSQL", lambda: subprocess.run(
        ["docker", "exec", "axismundi-db", "pg_isready", "-U", "user", "-d", "axismundi"],
        cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    ).returncode == 0)
    wait_for("Garage", lambda: http_ready(f"http://localhost:{ports['GARAGE_ADMIN']}/health"))
    wait_for("Imagor", lambda: http_ready(f"http://localhost:{ports['IMAGOR']}"))
    print("All services ready!")
    print(f"PostgreSQL: {ports['POSTGRES']}")
    print(f"Garage S3 API: http://localhost:{ports['GARAGE']}")
    print(f"Imagor: http://localhost:{ports['IMAGOR']}")
    print(f"Lexurgy: http://localhost:{ports['LEXURGY']}")
    print(f"Axismundi: http://localhost:{ports['APP']}")
    print("Backend config: .dev/config.json")
    print("Now you can run: just dev-backend")


if __name__ == "__main__":
    main()
