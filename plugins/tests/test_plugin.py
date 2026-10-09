"""Hermetic packaging regressions; never starts a client or reads operator config."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("build_plugin", ROOT / "plugins/build_plugin.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
PACKAGE = builder.PACKAGE


class PluginTests(unittest.TestCase):
    def test_all_generated_files_match_sources(self):
        for relative, expected in builder.generated_files().items():
            with self.subTest(relative=relative):
                self.assertEqual((PACKAGE / relative).read_text(encoding="utf-8"), expected)

    def test_cargo_policy_fixture_matches_packaged_policy(self):
        fixture = ROOT / "crates/redisctl-mcp/tests/fixtures/plugin-read-only.toml"
        self.assertEqual(fixture.read_text(), builder.POLICY)
        self.assertEqual(fixture.read_text(), (PACKAGE / "read-only.toml").read_text())

    def test_native_skills_keep_canonical_body_and_frontmatter(self):
        for name in builder.SKILLS:
            source = (ROOT / f"crates/redisctl-mcp/skills/{name}/SKILL.md").read_text()
            generated = (PACKAGE / f"skills/{name}/SKILL.md").read_text()
            header, body = source[4:].split("\n---\n", 1)
            self.assertTrue(generated.startswith("---\n" + header + "\n---\n"))
            self.assertTrue(generated.endswith(body.lstrip("\n")))
            self.assertIn("Plugin safety and discovery", generated)
            self.assertIn("do not enable writes or bypass policy", generated)
            self.assertIn("Never request credentials", generated)

    def test_both_clients_launch_the_same_binary_and_policy(self):
        portable = json.loads((PACKAGE / "mcp.json").read_text())
        claude = json.loads((PACKAGE / ".mcp.json").read_text())
        self.assertEqual(portable["$schema"], "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json")
        self.assertEqual(set(portable["mcpServers"]), {"redisctl"})
        self.assertEqual(set(claude["mcpServers"]), {"redisctl"})
        left = portable["mcpServers"]["redisctl"]
        right = claude["mcpServers"]["redisctl"]
        self.assertEqual(left["command"], "redisctl-mcp")
        self.assertEqual(left["type"], "stdio")
        self.assertEqual(right["command"], left["command"])
        self.assertEqual(left["args"], [arg.replace("CLAUDE_PLUGIN_ROOT", "PLUGIN_ROOT") for arg in right["args"]])
        self.assertIn("--read-only=true", left["args"])
        self.assertIn("${PLUGIN_ROOT}/read-only.toml", left["args"])
        self.assertIn("cloud,enterprise,database,app", left["args"])
        self.assertNotIn("env", left)  # no stored credentials or inherited policy settings

    def test_identity_and_onboarding_agree(self):
        portable = json.loads((PACKAGE / "plugin.json").read_text())
        claude = json.loads((PACKAGE / ".claude-plugin/plugin.json").read_text())
        for key in ("name", "version", "description", "license", "author"):
            self.assertEqual(portable[key], claude[key])
        onboarding = portable["extensions"]["com.openai"]["onboardingSkill"]
        self.assertTrue((PACKAGE / onboarding).is_file())
        self.assertNotIn("hooks", portable)
        self.assertNotIn("hooks", claude)
        self.assertFalse((PACKAGE / "hooks").exists())

    def test_catalog_paths_resolve_inside_marketplace_root(self):
        catalogs = [ROOT / "plugins/.claude-plugin/marketplace.json", ROOT / "plugins/.agents/plugins/marketplace.json"]
        for path in catalogs:
            catalog = json.loads(path.read_text())
            self.assertEqual(catalog["name"], "redisctl-experimental")
            self.assertEqual(len(catalog["plugins"]), 1)
            entry = catalog["plugins"][0]
            source = entry["source"]
            relative = source["path"] if isinstance(source, dict) else source
            self.assertEqual((ROOT / "plugins" / relative).resolve(), PACKAGE.resolve())
            self.assertEqual(entry["name"], "redisctl-mcp")

    def test_generator_rejects_mismatched_source_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            skill = root / "crates/redisctl-mcp/skills/redisctl-setup/SKILL.md"
            skill.parent.mkdir(parents=True)
            skill.write_text("---\nname: unrelated\ndescription: synthetic\n---\nbody\n")
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                builder.generated_files(root)

    def test_generator_rejects_malformed_frontmatter(self):
        for content in ("name: redisctl-setup\nbody", "---\nname: redisctl-setup\nbody"):
            with self.subTest(content=content), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                skill = root / "crates/redisctl-mcp/skills/redisctl-setup/SKILL.md"
                skill.parent.mkdir(parents=True)
                skill.write_text(content)
                with self.assertRaisesRegex(ValueError, "Invalid skill frontmatter"):
                    builder.generated_files(root)

    def test_check_rejects_drift_and_missing_files_without_repairing_them(self):
        with tempfile.TemporaryDirectory(prefix="plugin generation fixture ") as directory:
            root = Path(directory)
            script = root / "plugins/build_plugin.py"
            script.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / "plugins/build_plugin.py", script)
            for name in builder.SKILLS:
                source = ROOT / f"crates/redisctl-mcp/skills/{name}/SKILL.md"
                target = root / source.relative_to(ROOT)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
            subprocess.run([sys.executable, str(script)], check=True, capture_output=True)
            package = root / "plugins/redisctl-mcp"
            drifted = package / "mcp.json"
            drifted.write_text('{"synthetic_drift": true}\n')
            missing = package / "skills/data-explorer/SKILL.md"
            missing.unlink()  # only this test's disposable generated fixture
            fixture = root / "crates/redisctl-mcp/tests/fixtures/plugin-read-only.toml"
            fixture.write_text('tier = "full"\n')
            before = {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in root.rglob("*") if p.is_file()}
            result = subprocess.run([sys.executable, str(script), "--check"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            for relative in (drifted.relative_to(root), missing.relative_to(root), fixture.relative_to(root)):
                self.assertIn(str(relative), result.stderr)
            self.assertFalse(missing.exists())
            self.assertEqual(before, {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in before})

    def test_check_command_passes_without_rewriting_files(self):
        before = {p: p.stat().st_mtime_ns for p in PACKAGE.rglob("*") if p.is_file()}
        subprocess.run([sys.executable, str(ROOT / "plugins/build_plugin.py"), "--check"], check=True, capture_output=True)
        self.assertEqual(before, {p: p.stat().st_mtime_ns for p in before})


if __name__ == "__main__":
    unittest.main()
