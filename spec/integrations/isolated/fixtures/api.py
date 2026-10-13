"""Authored resettable target; upstream Schemathesis writes both summary and reproducers."""
import http.server,json,subprocess,sys,threading
class API(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self):
        if self.path != '/value': self.send_response(404);self.end_headers();return
        broken=json.load(open('/input/app.json'))['broken_api']
        self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers()
        self.wfile.write(json.dumps({'value':'wrong type' if broken else 12}).encode())
server=http.server.ThreadingHTTPServer(('127.0.0.1',8124),API)
threading.Thread(target=server.serve_forever,daemon=True).start()
try:
    result=subprocess.run(['/opt/python/bin/schemathesis','run','/input/openapi.json','--url','http://127.0.0.1:8124','--phases','fuzzing','--checks','not_a_server_error,status_code_conformance,response_schema_conformance','--mode','positive','--seed','17','--max-examples','8','--workers','1','--report','json,har','--report-json-path','/tmp/api.json','--report-har-path','/tmp/api.har','--generation-database','none','--no-color'],stdout=sys.stderr,stderr=sys.stderr)
    report=json.load(open('/tmp/api.json'))
    report['thruline_reproducers']=json.load(open('/tmp/api.har'))
    print(json.dumps(report))
    sys.exit(result.returncode)
finally: server.shutdown()
