#!/usr/bin/env python3
"""Container-only loopback HTTP transport. It generates no model responses."""
import base64
import argparse
import hashlib
import http.server
import json
from pathlib import Path
import subprocess
import sys
import threading
import time
import uuid

MAILBOX = Path('/mailbox')
p = argparse.ArgumentParser()
p.add_argument('--local-model-port', type=int, choices=[11435, 11436], required=True,
               help='The explicit accepted plan port served through this fixed mailbox.')
p.add_argument('command', nargs=argparse.REMAINDER)
a = p.parse_args()
if not a.command:
    raise SystemExit('guest command is required after the explicit model port')


def write(path, data):
    temporary = path.with_suffix('.new')
    temporary.write_bytes(data)
    temporary.replace(path)


class Handler(http.server.BaseHTTPRequestHandler):
    def exchange(self):
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if length < 0 or length > 2 * 1024 * 1024 or self.headers.get('Transfer-Encoding'):
                raise ValueError('unsupported request body framing')
            body = self.rfile.read(length)
            identity = uuid.uuid4().hex
            value = {'id': identity, 'method': self.command, 'path': self.path,
                     'headers': list(self.headers.items()), 'body_base64': base64.b64encode(body).decode(),
                     'body_sha256': hashlib.sha256(body).hexdigest()}
            write(MAILBOX/(identity+'.request.json'), json.dumps(value).encode())
            response = MAILBOX/(identity+'.response.json')
            deadline = time.monotonic() + 180
            while not response.is_file():
                if time.monotonic() > deadline:
                    raise TimeoutError('fixed host model broker did not respond')
                time.sleep(.03)
            result = json.loads(response.read_bytes())
            raw = base64.b64decode(result['body_base64'], validate=True)
            if hashlib.sha256(raw).hexdigest() != result['body_sha256']:
                raise ValueError('response bytes do not match broker hash')
            self.send_response(result['status'])
            for key, value in result['headers']:
                if key.lower() not in ('connection', 'transfer-encoding', 'content-length', 'server', 'date'):
                    self.send_header(key, value)
            self.send_header('Content-Length', str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
        except Exception as error:
            self.send_error(502, str(error))
    do_POST = exchange
    do_GET = exchange


server = http.server.ThreadingHTTPServer(('127.0.0.1', a.local_model_port), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
result = subprocess.run(a.command)
server.shutdown()
raise SystemExit(result.returncode)
