import asyncio
import sys
import tempfile
import unittest
from importlib.util import find_spec
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(
    find_spec("mcp"), "Install the optional mcp dependency to test its stdio adapter"
)
class MCPTests(unittest.TestCase):
    def test_stdio_render_and_output_boundary(self):
        from mcp import ClientSession, StdioServerParameters
        from mcp.client.stdio import stdio_client

        async def scenario(directory):
            root = Path(directory)
            output_dir = root / "allowed"
            parameters = StdioServerParameters(
                command=sys.executable,
                args=[str(ROOT / "mcp_server.py"), "--output-dir", str(output_dir)],
                cwd=str(root),
            )
            async with stdio_client(parameters) as streams:
                async with ClientSession(*streams) as session:
                    await session.initialize()
                    names = {tool.name for tool in (await session.list_tools()).tools}
                    self.assertEqual(names, {"list_assets", "render_handwriting"})
                    assets = await session.call_tool("list_assets", {})
                    self.assertFalse(assets.isError)
                    result = await session.call_tool(
                        "render_handwriting",
                        {"text": "你好 MCP", "output_path": "nested/hello.png"},
                    )
                    self.assertFalse(result.isError, result)
                    output = output_dir / "nested/hello.png"
                    self.assertTrue(output.is_file())
                    original = output.read_bytes()
                    duplicate = await session.call_tool(
                        "render_handwriting", {"text": "replace", "output_path": "nested/hello.png"}
                    )
                    self.assertTrue(duplicate.isError)
                    self.assertEqual(output.read_bytes(), original)
                    escaped = await session.call_tool(
                        "render_handwriting", {"text": "escape", "output_path": "../outside.png"}
                    )
                    self.assertTrue(escaped.isError)
                    self.assertFalse((root / "outside.png").exists())

        with tempfile.TemporaryDirectory() as directory:
            asyncio.run(asyncio.wait_for(scenario(directory), timeout=30))


if __name__ == "__main__":
    unittest.main()
