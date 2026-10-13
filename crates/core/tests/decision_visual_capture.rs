use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::json;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_visual::{
    check_visual_with, CaptureReceipt, VisualRuntime, APPROVED_RENDERER_SHA256,
};
use thruline_core::decisions::params::{parse_params, CheckParams};

const CLOCK: &str = "2026-10-06T13:43:10Z";
const RENDERER_VERSION: &str = "Chrome/154.0.8037.98";

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Server(std::process::Child, u16);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn serve(dir: &std::path::Path, pages: &Value) -> Server {
    let pages_path = dir.join("pages.json");
    std::fs::write(&pages_path, pages.to_string()).unwrap();
    let script = r#"from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json,sys
from urllib.parse import urlsplit
pages=json.load(open(sys.argv[1]))
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        with open(sys.argv[2],'a') as seen: seen.write(self.path+'\n')
        if self.path=='/favicon.ico':
            self.send_response(int(pages.get('favicon_status',204)))
            self.send_header('Content-Length','0')
            self.end_headers()
            return
        body=pages.get(urlsplit(self.path).path,pages['/']).encode()
        self.send_response(200)
        self.send_header('Content-Type','text/html')
        self.send_header('Content-Length',str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self,*args): pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
print(server.server_address[1],flush=True)
server.serve_forever()"#;
    let mut process = Command::new("python3")
        .args(["-u", "-c", script])
        .arg(pages_path)
        .arg(dir.join("requests.log"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut port = String::new();
    BufReader::new(process.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    Server(process, port.trim().parse().unwrap())
}

fn chrome() -> PathBuf {
    let path = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    assert_eq!(
        hash(&std::fs::read(&path).unwrap()),
        APPROVED_RENDERER_SHA256,
        "Chrome changed from the C28 pin"
    );
    path
}

fn captures(dir: &std::path::Path, port: u16, routes: &[(&str, &str)], fixture: &str) -> Value {
    captures_with_progress(dir, port, routes, fixture).0
}

fn captures_with_progress(
    dir: &std::path::Path,
    port: u16,
    routes: &[(&str, &str)],
    fixture: &str,
) -> (Value, String) {
    captures_with_paths(dir, port, routes, fixture, None, None)
}

fn captures_with_paths(
    dir: &std::path::Path,
    port: u16,
    routes: &[(&str, &str)],
    fixture: &str,
    allowed_paths: Option<&[&str]>,
    housekeeping_paths: Option<&[&str]>,
) -> (Value, String) {
    std::fs::create_dir_all(dir).unwrap();
    let profile = dir.join("profile.jsonl");
    std::fs::write(&profile, "").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let jobs = routes.iter().map(|(screen, route)| json!({"screen": screen,"route":route,"scenario":"fixed-record","width":320,"height":240,"full_page":true,"output":dir.join(format!("{screen}.png"))})).collect::<Vec<_>>();
    let input = dir.join("input.json");
    std::fs::write(&input, json!({"origin":format!("http://127.0.0.1:{port}"),"renderer":chrome(),"renderer_version":RENDERER_VERSION,"capture_clock":CLOCK,"fixture_sha256":fixture,"profile_path":profile,"allowed_paths":allowed_paths,"housekeeping_paths":housekeeping_paths,"jobs":jobs}).to_string()).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/decisions/visual_capture.mjs");
    let result = Command::new("node")
        .arg(script)
        .arg(input)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let progress = String::from_utf8(result.stderr).unwrap();
    eprintln!("{progress}");
    (serde_json::from_slice(&result.stdout).unwrap(), progress)
}

fn page(root_style: &str, content: &str) -> String {
    format!("<!doctype html><html><head><link rel=\"icon\" href=\"data:,\"><style>html,body{{margin:0;width:320px;height:240px}}#root{{width:320px;{root_style}}}.artboard{{width:320px;min-height:240px;background:rgb(0,255,0)}}</style></head><body><div id=\"root\"><div class=\"artboard\">{content}</div></div></body></html>")
}

fn geometry(shot: &Value, fixture: &str) {
    assert!(shot["error"].is_null(), "{shot}");
    assert_eq!(shot["capture_clock"], CLOCK);
    assert_eq!(shot["fixture_sha256"], fixture);
    assert_eq!(shot["renderer_version"], RENDERER_VERSION);
    assert_eq!(shot["frame_width"], 320);
    assert_eq!(shot["frame_height"], 240);
    let dimensions = Command::new("python3")
        .args([
            "-c",
            "from PIL import Image\nimport sys\nassert Image.open(sys.argv[1]).size == (320,240)",
        ])
        .arg(shot["screenshot_path"].as_str().unwrap())
        .output()
        .unwrap();
    assert!(
        dimensions.status.success(),
        "{}",
        String::from_utf8_lossy(&dimensions.stderr)
    );
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_DATE", CLOCK)
        .env("GIT_COMMITTER_DATE", CLOCK)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn verdict(shot: &Value, reference: &Value, fixture: &str) -> VerdictState {
    let repo = tempfile::tempdir().unwrap();
    let reference_bytes = std::fs::read(reference["screenshot_path"].as_str().unwrap()).unwrap();
    std::fs::write(repo.path().join("reference.png"), &reference_bytes).unwrap();
    git(repo.path(), &["init", "-q"]);
    git(repo.path(), &["add", "-A"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);
    let screen = shot["screen"].as_str().unwrap();
    let CheckParams::VisualMatch(params) = parse_params("visual_match",1,&json!({"kind":"visual_match","screens":[{"name":screen,"route":"/fixture","scenario":"fixed-record","reference_path":"reference.png","reference_sha256":hash(&reference_bytes),"width":320,"height":240,"full_page":true,"mask_rectangles":[]}],"tolerance_pct":0}).to_string()).unwrap() else { panic!("not visual") };
    let receipt = CaptureReceipt {
        screen: screen.into(),
        route: "/fixture".into(),
        scenario: "fixed-record".into(),
        actual_route: None,
        actual_scenario: None,
        commit_oid: commit.clone(),
        tree_hash: tree,
        renderer_sha256: APPROVED_RENDERER_SHA256.into(),
        renderer_version: shot["renderer_version"].as_str().map(str::to_owned),
        housekeeping_responses: serde_json::from_value(shot["housekeeping_responses"].clone())
            .unwrap(),
        capture_clock: shot["capture_clock"].as_str().map(str::to_owned),
        fixture_sha256: shot["fixture_sha256"].as_str().map(str::to_owned),
        frame_width: shot["frame_width"].as_u64().map(|n| n as u32),
        frame_height: shot["frame_height"].as_u64().map(|n| n as u32),
        content_width: shot["content_width"].as_u64().map(|n| n as u32),
        content_height: shot["content_height"].as_u64().map(|n| n as u32),
        overflow_x: shot["overflow_x"].as_bool(),
        overflow_y: shot["overflow_y"].as_bool(),
        scroll_regions: serde_json::from_value(shot["scroll_regions"].clone()).unwrap(),
        clipped_regions: serde_json::from_value(shot["clipped_regions"].clone()).unwrap(),
        route_found: true,
        screenshot_path: shot["screenshot_path"].as_str().map(PathBuf::from),
        failure: None,
    };
    check_visual_with(
        repo.path(),
        &commit,
        &params,
        &VisualRuntime {
            reference_root: repo.path().into(),
            renderer_sha256: Some(APPROVED_RENDERER_SHA256.into()),
            renderer_version: Some(RENDERER_VERSION.into()),
            capture_clock: Some(CLOCK.into()),
            fixture_sha256: Some(fixture.into()),
            captures: vec![receipt],
            diff_root: None,
        },
    )
    .outcome
    .state
}

#[test]
#[ignore = "requires the C28 pinned local Chrome"]
fn exact_frame_overflow_breaks_even_with_identical_visible_pixels_and_missing_section_differs() {
    let temp = tempfile::tempdir().unwrap();
    let pages = json!({"/":page("height:240px", ""),"/overflow":page("height:600px", "<div style=\"height:600px\"></div>"),"/hidden":page("height:240px;overflow:hidden", "<div style=\"height:600px\"></div>"),"/section":page("height:240px", "<section style=\"position:absolute;left:20px;top:120px;width:200px;height:40px;background:rgb(0,0,255)\"></section>"),"/missing":page("height:240px", "")});
    let fixture = hash(pages.to_string().as_bytes());
    let server = serve(temp.path(), &pages);
    let shots = captures(
        &temp.path().join("capture"),
        server.1,
        &[
            ("Normal", "/"),
            ("Overflow", "/overflow"),
            ("Hidden", "/hidden"),
            ("Section", "/section"),
            ("Missing", "/missing"),
        ],
        &fixture,
    );
    for shot in shots.as_array().unwrap() {
        geometry(shot, &fixture);
    }
    let reference = &shots[0];
    assert_eq!(verdict(reference, reference, &fixture), VerdictState::Holds);
    for shot in [&shots[1], &shots[2]] {
        assert_eq!(
            hash(&std::fs::read(shot["screenshot_path"].as_str().unwrap()).unwrap()),
            hash(&std::fs::read(reference["screenshot_path"].as_str().unwrap()).unwrap()),
            "visible pixels must match"
        );
        assert_eq!(shot["overflow_y"], true);
        assert!(shot["content_height"].as_u64().unwrap() >= 600);
        assert_eq!(verdict(shot, reference, &fixture), VerdictState::Broken);
    }
    assert_eq!(shots[4]["overflow_y"], false);
    assert_eq!(
        verdict(&shots[4], &shots[3], &fixture),
        VerdictState::Broken
    );
}

#[test]
#[ignore = "requires the C28 pinned local Chrome"]
fn designed_scroll_pane_is_recorded_without_expanding_the_artboard() {
    let temp = tempfile::tempdir().unwrap();
    let pages = json!({"/":page("height:240px", "<div style=\"position:absolute;left:20px;top:20px;width:200px;height:100px;overflow:auto\"><div style=\"height:500px;background:rgb(0,0,255)\"></div></div>")});
    let fixture = hash(pages.to_string().as_bytes());
    let server = serve(temp.path(), &pages);
    let shots = captures(
        &temp.path().join("capture"),
        server.1,
        &[("Scroll", "/")],
        &fixture,
    );
    geometry(&shots[0], &fixture);
    assert_eq!(shots[0]["overflow_y"], false);
    assert_eq!(shots[0]["content_height"], 240);
    assert_eq!(shots[0]["scroll_regions"].as_array().unwrap().len(), 1);
    assert_eq!(shots[0]["scroll_regions"][0]["content_height"], 500);
    assert_eq!(verdict(&shots[0], &shots[0], &fixture), VerdictState::Holds);
}

#[test]
#[ignore = "requires the C28 pinned local Chrome"]
fn date_is_pinned_before_page_script_with_native_call_shape_and_advancing_timers() {
    let temp = tempfile::tempdir().unwrap();
    let html = format!("{}<script>(()=>{{const screen=document.querySelector('.artboard');try{{const first=Date.now();const elapsed=performance.now();const good=first===1791294190000&&+new Date()===first&&Date()===new Date().toString()&&new Date(0).getTime()===0&&Date.parse('1970-01-01T00:00:00Z')===0&&Date.UTC(1970,0,1)===0&&new Date() instanceof Date&&(new Date()).constructor===Date;let ticks=0;setInterval(()=>{{ticks++;if(Date.now()!==first)screen.style.background='red'}},100);setTimeout(()=>{{const ok=good&&ticks>=5&&performance.now()-elapsed>=500;screen.style.background=ok?'rgb(0,255,0)':'red';screen.textContent=ok?'clock pinned; timer and performance advance':'clock error'}},700)}}catch(e){{screen.style.background='red';screen.textContent='Date call failed'}}}})()</script>",page("height:240px",""));
    let pages = json!({"/":html});
    let fixture = hash(pages.to_string().as_bytes());
    let server = serve(temp.path(), &pages);
    let first = captures(
        &temp.path().join("first"),
        server.1,
        &[("Clock", "/")],
        &fixture,
    );
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let second = captures(
        &temp.path().join("second"),
        server.1,
        &[("Clock", "/")],
        &fixture,
    );
    for shot in [&first[0], &second[0]] {
        geometry(shot, &fixture);
        let pixel=Command::new("python3").args(["-c","from PIL import Image\nimport sys\nassert Image.open(sys.argv[1]).convert('RGB').getpixel((160,120)) == (0,255,0)"]).arg(shot["screenshot_path"].as_str().unwrap()).output().unwrap();
        assert!(
            pixel.status.success(),
            "Date or advancing timer check failed"
        );
    }
    assert_eq!(
        hash(&std::fs::read(first[0]["screenshot_path"].as_str().unwrap()).unwrap()),
        hash(&std::fs::read(second[0]["screenshot_path"].as_str().unwrap()).unwrap())
    );
    let profile = std::fs::read_to_string(temp.path().join("first/profile.jsonl")).unwrap();
    assert!(profile.contains("chrome_start") && profile.contains("capture_screen"));
}

#[test]
#[ignore = "requires the C28 pinned local Chrome"]
fn off_origin_resource_request_cannot_be_hidden_as_a_successful_capture() {
    let temp = tempfile::tempdir().unwrap();
    let pages = json!({
        "/":page("height:240px","<img src=\"https://example.invalid/blocked-resource.png\">"),
        "/api-page":format!("{}<script>fetch('/api/record').then(r=>r.text())</script>",page("height:240px","")),
        "/query-page":page("height:240px","<img src=\"/?unexpected=1\">"),
        "/allowed-page":format!("{}<script>fetch('/assets/data.json').then(r=>r.text()).then(()=>document.querySelector('.artboard').textContent='allowed artifact read')</script>",page("height:240px","")),
        "/assets/data.json":"{}",
        "/housekeeping-page":page("height:240px", "").replace("<link rel=\"icon\" href=\"data:,\">", "")
    });
    let fixture = hash(pages.to_string().as_bytes());
    let server = serve(temp.path(), &pages);
    let shots = captures(
        &temp.path().join("capture"),
        server.1,
        &[("Network", "/")],
        &fixture,
    );
    assert!(shots[0]["screenshot_path"].is_null());
    assert!(
        shots[0]["error"].as_str().unwrap().contains("request"),
        "{shots}"
    );
    assert_eq!(shots[0]["capture_clock"], CLOCK);
    assert_eq!(shots[0]["fixture_sha256"], fixture);
    for (screen, route, denied) in [
        ("Api", "/api-page", "/api/record"),
        ("Query", "/query-page", "/?unexpected=1"),
    ] {
        let (shots, _) = captures_with_paths(
            &temp.path().join(screen),
            server.1,
            &[(screen, route)],
            &fixture,
            Some(&[
                "/",
                "/api-page",
                "/query-page",
                "/allowed-page",
                "/assets/data.json",
            ]),
            None,
        );
        assert!(
            shots[0]["screenshot_path"].is_null(),
            "static closure must reject {denied}"
        );
        assert!(shots[0]["error"].as_str().unwrap().contains("request"));
        assert_eq!(shots[0]["capture_clock"], CLOCK);
        assert_eq!(shots[0]["fixture_sha256"], fixture);
        let requests = std::fs::read_to_string(temp.path().join("requests.log")).unwrap();
        assert!(
            !requests.lines().any(|request| request == denied),
            "denied request reached the server"
        );
    }
    let (shots, _) = captures_with_paths(
        &temp.path().join("allowed"),
        server.1,
        &[("Allowed", "/allowed-page")],
        &fixture,
        Some(&["/", "/allowed-page", "/assets/data.json"]),
        None,
    );
    geometry(&shots[0], &fixture);
    let requests = std::fs::read_to_string(temp.path().join("requests.log")).unwrap();
    assert!(requests
        .lines()
        .any(|request| request == "/assets/data.json"));
    let (shots, _) = captures_with_paths(
        &temp.path().join("housekeeping"),
        server.1,
        &[("Housekeeping", "/housekeeping-page")],
        &fixture,
        Some(&["/", "/housekeeping-page"]),
        Some(&["/favicon.ico"]),
    );
    geometry(&shots[0], &fixture);
    assert_eq!(
        shots[0]["housekeeping_responses"],
        json!([{"path":"/favicon.ico","status":204}])
    );
    let bad = tempfile::tempdir().unwrap();
    let mut bad_pages = pages.clone();
    bad_pages["favicon_status"] = json!(200);
    let bad_server = serve(bad.path(), &bad_pages);
    let (shots, _) = captures_with_paths(
        &bad.path().join("capture"),
        bad_server.1,
        &[("BadHousekeeping", "/housekeeping-page")],
        &hash(bad_pages.to_string().as_bytes()),
        Some(&["/", "/housekeeping-page"]),
        Some(&["/favicon.ico"]),
    );
    assert!(
        shots[0]["screenshot_path"].is_null(),
        "housekeeping status must be verified"
    );
    assert!(shots[0]["error"].as_str().unwrap().contains("request"));
}

#[test]
#[ignore = "requires the C28 pinned local Chrome"]
fn four_tabs_reach_record_barrier_keep_order_and_isolate_capture_failure() {
    let temp = tempfile::tempdir().unwrap();
    let barrier_page = format!(
        "{}<script>const stamp=Date.now();fetch('/barrier?id='+location.pathname.slice(1)).then(r=>r.json()).then(r=>{{const ok=r.ready&&stamp===1791294190000&&Date.now()===stamp&&+new Date()===stamp;document.querySelector('.artboard').style.background=ok?'rgb(0,255,0)':'red'}})</script>",
        page("height:240px", "")
    );
    let pages = json!({"/":barrier_page,"/failure":page("height:240px","<img src=\"https://example.invalid/blocked-resource.png\">")});
    let pages_path = temp.path().join("barrier-pages.json");
    std::fs::write(&pages_path, pages.to_string()).unwrap();
    let script = r#"from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import urlsplit,parse_qs
import json,sys,time,threading
pages=json.load(open(sys.argv[1]))
seen=set()
condition=threading.Condition()
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url=urlsplit(self.path)
        if url.path=='/barrier':
            key=parse_qs(url.query).get('id',[''])[0]
            with condition:
                if key in ('1','2','3','4'): seen.add(key)
                condition.notify_all()
                deadline=time.monotonic()+5
                while len(seen)<4 and time.monotonic()<deadline:
                    condition.wait(max(0,deadline-time.monotonic()))
                body=json.dumps({'ready':len(seen)==4}).encode()
            kind='application/json'
        else:
            body=pages.get(url.path,pages['/']).encode()
            kind='text/html'
        self.send_response(200)
        self.send_header('Content-Type',kind)
        self.send_header('Content-Length',str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self,*args): pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
print(server.server_address[1],flush=True)
server.serve_forever()"#;
    let fixture = hash(format!("{script}{}", pages).as_bytes());
    for failed_job_first in [false, true] {
        let mut process = Command::new("python3")
            .args(["-u", "-c", script])
            .arg(&pages_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut port = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let server = Server(process, port.trim().parse().unwrap());
        let mut routes = vec![
            ("Tab1", "/1"),
            ("Tab2", "/2"),
            ("Tab3", "/3"),
            ("Tab4", "/4"),
        ];
        if failed_job_first {
            routes.insert(0, ("Failed", "/failure"));
        }
        let dir = temp.path().join(if failed_job_first {
            "failure"
        } else {
            "parallel"
        });
        let (shots, progress) = captures_with_progress(&dir, server.1, &routes, &fixture);
        assert_eq!(shots.as_array().unwrap().len(), routes.len());
        for (index, (name, _)) in routes.iter().enumerate() {
            let shot = &shots[index];
            assert_eq!(shot["screen"], *name, "results must stay in input order");
            if *name == "Failed" {
                assert!(shot["screenshot_path"].is_null());
                assert!(shot["error"].as_str().unwrap().contains("request"));
                assert_eq!(shot["capture_clock"], CLOCK);
                assert_eq!(shot["fixture_sha256"], fixture);
                assert_eq!(shot["renderer_version"], RENDERER_VERSION);
                assert_eq!(shot["frame_width"], 320);
                assert_eq!(shot["frame_height"], 240);
                continue;
            }
            // The first serial tab cannot turn green: its request times out
            // before the other three tabs can reach the same record barrier.
            let pixel = Command::new("python3")
                .args(["-c", "from PIL import Image\nimport sys\nassert Image.open(sys.argv[1]).convert('RGB').getpixel((160,120)) == (0,255,0)"])
                .arg(shot["screenshot_path"].as_str().unwrap())
                .output()
                .unwrap();
            assert!(
                pixel.status.success(),
                "all four tabs must reach the record barrier"
            );
            geometry(shot, &fixture);
        }
        let expected = (1..=routes.len())
            .map(|n| format!("Captured {n} of {}", routes.len()))
            .collect::<Vec<_>>();
        assert_eq!(progress.lines().collect::<Vec<_>>(), expected);
        let profile = std::fs::read_to_string(dir.join("profile.jsonl")).unwrap();
        let stages = profile
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            stages
                .iter()
                .filter(|stage| stage["stage"] == "chrome_start")
                .count(),
            1
        );
        assert_eq!(
            stages
                .iter()
                .filter(|stage| stage["stage"] == "capture_screen")
                .count(),
            routes.len()
        );
    }
}

#[test]
#[ignore = "requires pinned local Chrome"]
fn capture_waits_for_a_delayed_record_read_to_settle() {
    let renderer = chrome();
    let server_script = r#"from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import time
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/data':
            time.sleep(1.8)
            body, kind = b'{"ready":true}', 'application/json'
        else:
            body = b'''<!doctype html><html><body style="margin:0"><div id="root"><div id="screen" style="width:320px;height:240px;background:rgb(255,0,0)">pending</div></div><script>fetch('/data').then(r=>r.json()).then(()=>{document.getElementById('screen').style.background='rgb(0,255,0)';document.getElementById('screen').textContent='ready'})</script></body></html>'''
            kind = 'text/html'
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()"#;
    let mut server = Command::new("python3")
        .args(["-u", "-c", server_script])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("capture.png");
    let input = temp.path().join("input.json");
    std::fs::write(
        &input,
        json!({
            "origin": format!("http://127.0.0.1:{port}"),
            "capture_clock": "2026-10-06T13:43:10Z",
            "fixture_sha256": hash(server_script.as_bytes()),
            "renderer": renderer,
            "renderer_version": RENDERER_VERSION,
            "jobs": [{
                "screen": "slow-record", "route": "#/slow-record", "scenario": "fixed-record",
                "width": 320, "height": 240, "full_page": false, "output": output
            }]
        })
        .to_string(),
    )
    .unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/decisions/visual_capture.mjs");
    let result = Command::new("node")
        .arg(script)
        .arg(&input)
        .output()
        .unwrap();
    let _ = server.kill();
    let _ = server.wait();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipts: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(receipts[0]["error"].is_null(), "{receipts}");
    let pixel = Command::new("python3")
        .args(["-c", "from PIL import Image\nimport sys\npixel=Image.open(sys.argv[1]).convert('RGB').getpixel((160,120))\nprint(pixel)\nassert pixel == (0,255,0)"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        pixel.status.success(),
        "capture kept the pending screen: {} {}",
        String::from_utf8_lossy(&pixel.stdout),
        String::from_utf8_lossy(&pixel.stderr)
    );
}
