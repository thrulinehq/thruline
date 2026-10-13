#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data:&[u8]|{
 if data.len()>1024*1024{return;}
 for format in ["ast-grep-json-v1","cargo-deny-json-v1","semgrep-json-v1","sarif-2.1.0","cargo-test-text-v1"] {
  let _=thruline_verify::integrations::decode(format,data);
 }
});
