// Authored fixture server + the real Playwright test runner, container loopback only.
const http = require('node:http'), fs = require('node:fs'), { spawn } = require('node:child_process');
let records = [];
const broken = JSON.parse(fs.readFileSync('/input/app.json')).broken_browser;
const html = `<form><label>Amount<input aria-label="Amount" type="number" /></label><button>Save</button></form><p role="alert"></p><ul></ul><script>
async function load(){const rows=await(await fetch('/records')).json();document.querySelector('ul').textContent=rows.join(', ')}
document.querySelector('form').onsubmit=async e=>{e.preventDefault();const n=Number(document.querySelector('input').value);if(n<=0){document.querySelector('[role=alert]').textContent='Enter a positive amount';return;}document.querySelector('[role=alert]').textContent='';await fetch('/records',{method:'POST',body:JSON.stringify(n)});load()};load();</script>`;
const server = http.createServer((req, res) => {
  if(req.url==='/records' && req.method==='POST') { let body='';req.on('data',x=>body+=x);req.on('end',()=>{records.push(JSON.parse(body));res.end('{}')});return; }
  if(req.url==='/records'){res.setHeader('content-type','application/json');res.end(JSON.stringify(records));return;}
  if(broken)records=[];
  res.setHeader('content-type','text/html');res.end(html);
});
server.listen(8123,'127.0.0.1',()=>{
  if(process.env.THRULINE_SERVE_ONLY==='1') return;
  const child=spawn('/opt/tools/node_modules/.bin/playwright',['test','--config','/input/playwright.config.cjs'],{stdio:'inherit'});
  child.on('exit',code=>server.close(()=>process.exit(code??2)));
});
