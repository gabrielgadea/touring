"""S73 (26/09/2026): the ground-truth collector must ask the PROJECT's daemon.

Every Claude Code session exports TOURING_DAEMON_SOCKET, the GLOBAL daemon, and that
variable beats the per-project walk-up: inside a project with its own daemon (the
analise, `[daemon] per_project = true`) the collector's `touring` calls answered from
the wrong index. Inside such a project the subprocess env drops the two variables;
outside it keeps them.
"""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import lib  # noqa: E402

GLOBAL = {"TOURING_DAEMON_SOCKET": "/tmp/global.sock", "TOURING_DAEMON_SOCK": "/tmp/global.sock"}


class TouringEnvTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.project = root / "proj"
        (self.project / ".touring").mkdir(parents=True)
        (self.project / ".touring" / "touring.toml").write_text("[daemon]\nper_project = true\n")
        (self.project / "src" / "deep").mkdir(parents=True)
        self.elsewhere = root / "other"
        self.elsewhere.mkdir()

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_a_per_project_tree_drops_the_global_socket(self) -> None:
        with mock.patch.dict(os.environ, GLOBAL):
            env = lib.touring_env(self.project / "src" / "deep")
        self.assertNotIn("TOURING_DAEMON_SOCKET", env)
        self.assertNotIn("TOURING_DAEMON_SOCK", env)

    def test_outside_a_per_project_tree_the_env_is_untouched(self) -> None:
        with mock.patch.dict(os.environ, GLOBAL):
            env = lib.touring_env(self.elsewhere)
        self.assertEqual(env["TOURING_DAEMON_SOCKET"], "/tmp/global.sock")

    def test_only_a_real_boolean_opt_in_counts_as_the_executor_reads_it(self) -> None:
        # TouringConfig::daemon_per_project_opt_in (touring-foundation config/paths.rs)
        # parses the TOML and takes `as_bool`: a string or a malformed file is NO opt-in.
        toml = self.elsewhere / ".touring" / "touring.toml"
        toml.parent.mkdir()
        for body in ('[daemon]\nper_project = "true"\n', "# per_project = true\n", "[daemon\nper_project = true\n"):
            toml.write_text(body)
            with mock.patch.dict(os.environ, GLOBAL):
                env = lib.touring_env(self.elsewhere)
            self.assertIn("TOURING_DAEMON_SOCKET", env, body)
        (self.elsewhere / ".touring" / "daemon.sock").touch()
        with mock.patch.dict(os.environ, GLOBAL):
            env = lib.touring_env(self.elsewhere)
        self.assertNotIn("TOURING_DAEMON_SOCKET", env, "a bound socket claims the tree on its own")

    def test_every_touring_subprocess_gets_the_project_env(self) -> None:
        calls: list[tuple[list[str], dict]] = []

        def fake_run(*args, **kwargs):
            calls.append((args[0], kwargs))
            return subprocess.CompletedProcess(args[0], 0, stdout="{}", stderr="")

        cwd = Path.cwd()
        try:
            os.chdir(self.project / "src")
            with mock.patch.dict(os.environ, GLOBAL), mock.patch.object(lib.subprocess, "run", side_effect=fake_run):
                lib.run_touring(["doctor", "-j"])
                lib.touring_memory_store("k", "v")
                lib.touring_learning_reward("tool", 1.0)
        finally:
            os.chdir(cwd)
        verbs = {argv[1] for argv, _ in calls}
        self.assertEqual(verbs, {"--version", "doctor", "memory", "learning"}, calls)
        for argv, kwargs in calls:
            self.assertIn("env", kwargs, f"{argv} must hand the subprocess an explicit env")
            self.assertNotIn("TOURING_DAEMON_SOCKET", kwargs["env"], argv)


if __name__ == "__main__":
    unittest.main()
