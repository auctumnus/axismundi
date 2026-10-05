"""Regression checks without starting Docker or changing the real dev config."""

from contextlib import redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("dev_services", REPO / "scripts/dev-services.py")
dev = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dev)


class PortTests(unittest.TestCase):
    def choose(self, owned):
        with tempfile.TemporaryDirectory() as directory:
            saved = Path(directory) / "ports.json"
            saved.write_text(json.dumps({"APP": 3002}))
            with (
                patch.object(dev, "PORTS_FILE", saved),
                patch.object(dev, "published_port", return_value=None),
                patch.object(dev, "port_is_free", side_effect=lambda p: p not in (3000, 3002)),
                patch.object(dev, "app_owns_port", side_effect=lambda p: owned and p == 3002),
                patch.object(dev, "port_owner", return_value="busy process"),
                redirect_stdout(io.StringIO()),
            ):
                return dev.select_ports()

    def test_refresh_reuses_this_backends_saved_port(self):
        self.assertEqual(self.choose(owned=True)["APP"], 3002)

    def test_unrelated_listener_gets_a_new_port(self):
        self.assertEqual(self.choose(owned=False)["APP"], 3003)

    def test_owner_requires_this_checkout_and_backend_executable(self):
        for cwd, executable, expected in [
            (REPO, REPO / "target/debug/axismundi", True),
            (REPO.parent / "other", REPO / "target/debug/axismundi", False),
            (REPO, REPO.parent / "other/target/debug/axismundi", False),
            (REPO, REPO / "target/debug/seed", False),
        ]:
            with self.subTest(cwd=cwd, executable=executable):
                def resolve(path, strict=False):
                    return cwd if path.name == "cwd" else executable

                with (
                    patch.object(dev, "port_processes", return_value=[("axismundi", "123")]),
                    patch.object(Path, "resolve", resolve),
                ):
                    self.assertEqual(dev.app_owns_port(3002), expected)


class CommandTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "scripts").mkdir()
        for name in ("dev-services.py", "pre-commit.sh"):
            shutil.copy2(REPO / "scripts" / name, self.root / "scripts" / name)
        shutil.copy2(REPO / "justfile", self.root / "justfile")
        (self.root / ".dev").mkdir()
        (self.root / ".dev/config.json").write_text(json.dumps({
            "database_url": "postgres://user:password@localhost:5433/axismundi",
            "public_url_base": "http://localhost:3002",
        }))
        fake_bin = self.root / "bin"
        fake_bin.mkdir()
        for name in ("cargo", "watchexec", "sqlx"):
            executable = fake_bin / name
            executable.write_text("#!/usr/bin/env python3\nimport json, os, sys\n"
                                  "print(json.dumps({'database_url': os.environ.get('DATABASE_URL'), 'args': sys.argv[1:]}))\n")
            executable.chmod(0o755)
        self.environment = {
            **os.environ,
            "PATH": f"{fake_bin}:{os.environ['PATH']}",
            "DATABASE_URL": "postgres://user:password@localhost:5432/axismundi",
        }

    def run_command(self, command):
        return subprocess.run(command, cwd=self.root, env=self.environment,
                              capture_output=True, text=True, check=True)

    def assert_generated_database(self, command):
        result = self.run_command(command)
        records = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        self.assertEqual(len(records), 1, result.stdout)
        self.assertEqual(records[0]["database_url"], "postgres://user:password@localhost:5433/axismundi")
        return records[0]

    def test_run_build_watch_and_migrations_override_stale_environment(self):
        for recipe in ("run", "build", "dev-backend", "db-migrate"):
            with self.subTest(recipe=recipe):
                record = self.assert_generated_database(["just", "--no-deps", recipe])
                if recipe in ("run", "dev-backend"):
                    self.assertEqual(record["args"][-1], ".dev/config.json")

    def test_run_uses_local_config_without_generated_config(self):
        (self.root / ".dev/config.json").unlink()
        (self.root / "config.json").write_text(json.dumps({
            "database_url": "postgres://user:password@localhost:5433/axismundi",
        }))
        record = self.assert_generated_database(["just", "--no-deps", "run"])
        self.assertEqual(record["args"][-1], "config.json")

    def test_pre_commit_uses_generated_database_for_sqlx(self):
        self.run_command(["git", "init", "-q"])
        (self.root / "changed.rs").write_text("// staged Rust change\n")
        self.run_command(["git", "add", "changed.rs"])
        # The hook hides cargo stdout, so record its environment in a file.
        cargo = self.root / "bin/cargo"
        cargo.write_text("#!/usr/bin/env python3\nimport os\n"
                         "open('cargo-database', 'w').write(os.environ['DATABASE_URL'])\n")
        self.run_command(["bash", "scripts/pre-commit.sh"])
        self.assertEqual((self.root / "cargo-database").read_text(),
                         "postgres://user:password@localhost:5433/axismundi")


if __name__ == "__main__":
    unittest.main()
