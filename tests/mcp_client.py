"""Run after cargo build: python tests/mcp_client.py [profile.json | --local]."""
import base64
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
exe = root / 'target' / 'debug' / ('aspectwrite.exe' if os.name == 'nt' else 'aspectwrite')
fixture = {'schema': 'aspectwrite.handwriting', 'version': 1, 'glyphs': [
    {'key': 'x', 'status': 'complete', 'bbox': {'minX': 0, 'minY': 0, 'maxX': 40, 'maxY': 90},
     'strokes': [[{'x': 0, 'y': 90}, {'x': 40, 'y': 0}]]}]}
with tempfile.TemporaryDirectory() as tmp:
    local = len(sys.argv) > 1 and sys.argv[1] == '--local'
    profile = (root / '.local/aspectwrite-strokes.json') if local else (pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else pathlib.Path(tmp) / 'profile.json')
    if len(sys.argv) == 1:
        profile.write_text(json.dumps(fixture))
    env = dict(os.environ)
    env.pop('ASPECTWRITE_STROKES', None)
    if not local: env['ASPECTWRITE_STROKES'] = str(profile)
    proc = subprocess.Popen([str(exe), 'mcp'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=root)
    def call(id, method, params=None):
        msg = {'jsonrpc': '2.0', 'id': id, 'method': method}
        if params is not None: msg['params'] = params
        proc.stdin.write((json.dumps(msg) + '\n').encode())
        proc.stdin.flush()
        while True:
            line = proc.stdout.readline()
            assert line, proc.stderr.read().decode()
            response = json.loads(line)
            if response.get('id') == id:
                assert 'error' not in response, response
                return response['result']
    try:
        result = call(1, 'initialize', {'protocolVersion': '2025-03-26', 'capabilities': {}, 'clientInfo': {'name': 'aspectwrite-test', 'version': '1'}})
        assert 'capabilities' in result
        proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proc.stdin.flush()
        tool = next(t for t in call(2, 'tools/list')['tools'] if t['name'] == 'render_latex')
        assert 'CLOSED' in tool['description'] and 'xrightarrow' in tool['description']
        expressions = [p.read_text() for p in (root / 'examples').glob('*.tex')] if len(sys.argv) > 1 else ['xx']
        for n, latex in enumerate(expressions, 3):
            response = call(n, 'tools/call', {'name': 'render_latex', 'arguments': {'latex': latex, 'seed': 7}})
            assert not response.get('isError'), response
            image = response['content'][0]
            assert image['type'] == 'image' and image['mimeType'] == 'image/png'
            png = base64.b64decode(image['data'], validate=True)
            assert png[:8] == b'\x89PNG\r\n\x1a\n'
            width, height = struct.unpack('>II', png[16:24])
            assert width > 0 and height > 0
            output = pathlib.Path(tmp) / 'cli.png'
            subprocess.run([str(exe), 'render', str(profile), str(output), latex, '--seed', '7'], check=True, capture_output=True)
            assert output.read_bytes() == png, 'CLI and MCP PNG differ'
        n = 100
        response = call(100, 'tools/call', {'name': 'render_latex', 'arguments': {'latex': r'\unsupported{x}'}})
        assert response.get('isError') and 'unsupported command' in response['content'][0]['text'], response
        # A newer complete profile may include the formerly missing symbol.
        response = call(101, 'tools/call', {'name': 'render_latex', 'arguments': {'latex': r'x\mathbb{R}'}})
        keys = {g['key'] for g in json.loads(profile.read_text(encoding='utf-8'))['glyphs'] if g.get('status') == 'complete'}
        if r'\mathbb{R}' not in keys:
            assert response.get('isError') and 'missing handwriting glyph' in response['content'][0]['text'], response
        else:
            assert not response.get('isError') and response['content'][0]['type'] == 'image', response
    finally:
        proc.stdin.close()
        proc.wait(timeout=10)
print('MCP initialize/list/render/errors/CLI parity: OK')
