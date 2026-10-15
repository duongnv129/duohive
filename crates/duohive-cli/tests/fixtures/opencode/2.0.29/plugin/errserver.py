import http.server, json, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n=int(self.headers.get('content-length',0)); self.rfile.read(n)
        code=int(sys.argv[2]); body=json.dumps({"error":{"message":"spike: model not found","type":"invalid_request_error","code":"model_not_found"}}).encode()
        self.send_response(code); self.send_header('content-type','application/json'); self.send_header('content-length',str(len(body))); self.end_headers(); self.wfile.write(body)
        open(sys.argv[3],'a').write(f"POST {self.path} -> {code}\n")
    def do_GET(self): self.do_POST()
    def log_message(self,*a): pass
http.server.HTTPServer(('127.0.0.1',int(sys.argv[1])),H).serve_forever()
