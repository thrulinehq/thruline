//! A209-C9 — plumbing controls: which argument gets decoded, which slot
//! gets consumed, which character gets dropped.
//!
//! Ten rounds in, the detector, the payload test and the index map are all
//! sound. What keeps failing is wiring. Each control here was verified
//! FAILING against `b269981` before the fix landed.

use thruline_core::redact::mask_recorded;

/// Stock Node 22 renderer output. Not a contrived transport — this is what
/// `console.log` prints for an object holding a PEM file, and it is what
/// anyone debugging a service-account config in Node sees.
const NODE_CONSOLE_LOG: &str = "{\n  type: 'service_account',\n  private_key: '-----BEGIN PRIVATE KEY-----\\n' +\n    'MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds\\n' +\n    'HaQjEBFQyeodUcjRzeZLuNCEyHz0mSjrQBX6XhVsJ5cqGmG3rhYuH8l/8rvolp6m\\n' +\n    'mfQB//QHMoC6tcQUpeAw5kHQDywq4OwGFPZds8qylzTPQW7nU01q9b+oW15+yIy2\\n' +\n    'AWkBp1Z0UMMFN63UEcnG3ayPwqUdfTUcM7OeWd5iqxV0w2SXyzTyoztRx5e/FyU3\\n' +\n    'K0iN7gsKHVe04WpuXRQgr/XXUfMvzPjc+eT/aI3dEL/eMu0ymwOYp1xaDDtE0Cj/\\n' +\n    'F4FO+BMzho0/+hDxRevEVHQbiKnOhF2Ah4oVZS4+uL7WVttrdOX7E65zEGSc770A\\n' +\n    'kdskcAxNAgMBAAECggEAA4liFxqH35ICNNUaa7tmgXPkAdyraey1kFSA1VHbJnl6\\n' +\n    'EYrAAPNlGJVnbNxla3UvIoHkQwk79JdRgSHoTFOAcdTTJv/MuZJ1SKpXrypjL4zv\\n' +\n    'p6wmDSQgaKvBTIhVnaL4E+iRWyEonWesXd2IC4qO+poDKannr2wJvz9GCthwfbB7\\n' +\n    '8lzqKT64ZYFXL4FlvHOIpu/FQ/nDiBWadsdIvdKJApjVeNBaFNI+Fm9LXDLizgsw\\n' +\n    'ThMN9nXoH3eG9KkJHrUVUzvwoMuLcqbLMkSDCbMhacS6M8efb8vN5+HEE2o+orla\\n' +\n    'MWzw7lRIIkOpf7bsh2AL4rL2PuLluSRZHfOxUuOskQKBgQD+hfJIKckToAmeaLA7\\n' +\n    'wTujdAE9c6RZLjDUKNkEAurN3/vZSelVHUi9jN0+T9MhIuDLuq9pOf+7F48hjTdH\\n' +\n    '5f50WEGTQmm/wzTO5ZKivMZrucwG8JVm3ifQcTDpHQfhV0S3lqACMYNp0zvcgevp\\n' +\n    '0x6XnG0mSeMsvA3bsv+JQCyf6QKBgQC51QivRkRXNpfv4qH0uvT3vMlVxQgaK9wa\\n' +\n    'fhk6N0oVcmbcqngn2fDzkc55ME8rpkdspYidIfrQp5SCoJH8FFmel/1VCH+jxOYs\\n' +\n    'RkTVJBD7D5YAv3X3OlnbT/DXYTSN58sKqk+ICf+rndTidPtfUjmvC+7iY4KY50u8\\n' +\n    'ts5cYmBOxQKBgQCPZHM9CJ2J+u2wWxZ+c7zft2lDb7U1sPo9bM3mnqYeWoQOxL85\\n' +\n    '/oeOkp/KjTpLPtUashBE3jIe+4TdyCtOzFpwvZAkIJRnPtivf1L/cA7xffleJ4hk\\n' +\n    'LeCMZFjD3GEmKP5D2A5Jbg+1PRZKgr+gOhrQG4a6vYHJICFGly+z6X26kQKBgQCj\\n' +\n    'uq0SMOjZpoIrmxRy2Z4oAC+r4BOJa4db2CpF/pzJVKiIV7HR+tWu5JJ0eDnpDXH0\\n' +\n    'qcw6Fcw0ttHEXdEBtrRXZTaRCxHaqwrZi3fuCv7jFRJmykoXb1PoRQC/4yc4uDdO\\n' +\n    'yEuWYqje/gqpE8S+cWXdcrggwug36+Hw/WfIQez0uQKBgEh15dr+T4uR42dDdsb2\\n' +\n    'DukUnJ8XZgcjmRz6HIr6MKbq236q+qi4arqGyIYQtDUFio9Rjsh5uBd7fjow4oMw\\n' +\n    'xYCMo5fBhda5ERmDsKHYBsgPTHXtscYgzBHSFFIqht7UpnXtLSDGvt6beru19NHl\\n' +\n    'UfocB21M+zB+Rj4/f3UQilGJ\\n' +\n    '-----END PRIVATE KEY-----\\n'\n}";
/// PKCS#8 Ed25519: banner, ONE 64-character body line, banner.
const ED25519_PKCS8: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIEmq1VYG5R4XwsG6oh4CBBjal2nHntJQYo9/FOpnymM/\n-----END PRIVATE KEY-----";
/// PKCS#8 X25519: same single-line shape.
const X25519_PKCS8: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VuBCIEIGiRzJ5FoQkoYubWcC08PPDBEtoeoToJG/4rT9UmUmF1\n-----END PRIVATE KEY-----";
const RSA_2048: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds\nHaQjEBFQyeodUcjRzeZLuNCEyHz0mSjrQBX6XhVsJ5cqGmG3rhYuH8l/8rvolp6m\nmfQB//QHMoC6tcQUpeAw5kHQDywq4OwGFPZds8qylzTPQW7nU01q9b+oW15+yIy2\nAWkBp1Z0UMMFN63UEcnG3ayPwqUdfTUcM7OeWd5iqxV0w2SXyzTyoztRx5e/FyU3\nK0iN7gsKHVe04WpuXRQgr/XXUfMvzPjc+eT/aI3dEL/eMu0ymwOYp1xaDDtE0Cj/\nF4FO+BMzho0/+hDxRevEVHQbiKnOhF2Ah4oVZS4+uL7WVttrdOX7E65zEGSc770A\nkdskcAxNAgMBAAECggEAA4liFxqH35ICNNUaa7tmgXPkAdyraey1kFSA1VHbJnl6\nEYrAAPNlGJVnbNxla3UvIoHkQwk79JdRgSHoTFOAcdTTJv/MuZJ1SKpXrypjL4zv\np6wmDSQgaKvBTIhVnaL4E+iRWyEonWesXd2IC4qO+poDKannr2wJvz9GCthwfbB7\n8lzqKT64ZYFXL4FlvHOIpu/FQ/nDiBWadsdIvdKJApjVeNBaFNI+Fm9LXDLizgsw\nThMN9nXoH3eG9KkJHrUVUzvwoMuLcqbLMkSDCbMhacS6M8efb8vN5+HEE2o+orla\nMWzw7lRIIkOpf7bsh2AL4rL2PuLluSRZHfOxUuOskQKBgQD+hfJIKckToAmeaLA7\nwTujdAE9c6RZLjDUKNkEAurN3/vZSelVHUi9jN0+T9MhIuDLuq9pOf+7F48hjTdH\n5f50WEGTQmm/wzTO5ZKivMZrucwG8JVm3ifQcTDpHQfhV0S3lqACMYNp0zvcgevp\n0x6XnG0mSeMsvA3bsv+JQCyf6QKBgQC51QivRkRXNpfv4qH0uvT3vMlVxQgaK9wa\nfhk6N0oVcmbcqngn2fDzkc55ME8rpkdspYidIfrQp5SCoJH8FFmel/1VCH+jxOYs\nRkTVJBD7D5YAv3X3OlnbT/DXYTSN58sKqk+ICf+rndTidPtfUjmvC+7iY4KY50u8\nts5cYmBOxQKBgQCPZHM9CJ2J+u2wWxZ+c7zft2lDb7U1sPo9bM3mnqYeWoQOxL85\n/oeOkp/KjTpLPtUashBE3jIe+4TdyCtOzFpwvZAkIJRnPtivf1L/cA7xffleJ4hk\nLeCMZFjD3GEmKP5D2A5Jbg+1PRZKgr+gOhrQG4a6vYHJICFGly+z6X26kQKBgQCj\nuq0SMOjZpoIrmxRy2Z4oAC+r4BOJa4db2CpF/pzJVKiIV7HR+tWu5JJ0eDnpDXH0\nqcw6Fcw0ttHEXdEBtrRXZTaRCxHaqwrZi3fuCv7jFRJmykoXb1PoRQC/4yc4uDdO\nyEuWYqje/gqpE8S+cWXdcrggwug36+Hw/WfIQez0uQKBgEh15dr+T4uR42dDdsb2\nDukUnJ8XZgcjmRz6HIr6MKbq236q+qi4arqGyIYQtDUFio9Rjsh5uBd7fjow4oMw\nxYCMo5fBhda5ERmDsKHYBsgPTHXtscYgzBHSFFIqht7UpnXtLSDGvt6beru19NHl\nUfocB21M+zB+Rj4/f3UQilGJ\n-----END PRIVATE KEY-----";

fn leaks(doc: &str, needle: &str) -> bool {
    let (out, rec) = mask_recorded(doc);
    !rec.families.contains("private_key_block") || out.contains(needle)
}

/// BLOCKING 1: the decoder saw `same_line_tail` only. When the banner ends
/// a physical line and the escaped body starts on the NEXT one, the
/// following lines arrived raw, kept their literal backslashes and quotes,
/// and the payload test rejected every one.
#[test]
fn b10_1_node_console_log_output_is_masked() {
    let frag = "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds";
    assert!(NODE_CONSOLE_LOG.contains(frag), "fixture lost its payload");
    assert!(
        !leaks(NODE_CONSOLE_LOG, frag),
        "stock Node console.log output shipped an RSA-2048 key verbatim"
    );
}

/// The same defect, minimised: banner ends the line, body escaped below.
#[test]
fn b10_1b_banner_ends_line_with_escaped_body_below() {
    let doc = format!(
        "private_key:\n  '-----BEGIN PRIVATE KEY-----\\n' +\n  '{}\\n' +\n  '-----END PRIVATE KEY-----\\n'",
        "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds"
    );
    assert!(
        !leaks(&doc, "MIIEvgIBADANBgkqhkiG"),
        "an escaped body on the following line was never decoded"
    );
}

/// BLOCKING 2: window starvation. Decoded same-line segments were pushed
/// first and the window truncated at 13, so a banner-line tail carrying 12
/// or more recognised breaks filled every slot with empty strings and the
/// real body below was never examined. 9/10/11 masked; 12/13/14 leaked.
#[test]
fn b10_2_a_break_heavy_banner_tail_does_not_starve_the_window() {
    let body = "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds";
    for (name, filler) in [
        ("unknown-escape", "\\q"),
        ("percent-lf", "%0A"),
        ("escaped-tab", "\\t"),
        ("windows-path", "C:\\tmp\\re"),
    ] {
        for count in [12usize, 13, 14, 20] {
            let doc = format!(
                "-----BEGIN PRIVATE KEY-----{}\n{body}\n-----END PRIVATE KEY-----",
                filler.repeat(count)
            );
            assert!(
                !leaks(&doc, body),
                "{name} x{count} in the banner tail starved the window and shipped the key"
            );
        }
    }
    // The realistic shell-transcript shape: a cat command, then the banner.
    let doc = format!(
        "cat /etc/ssl/private/app.key && echo done\n-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----"
    );
    assert!(!leaks(&doc, body), "shell-transcript shape shipped the key");
}

/// BLOCKING 3: the unknown-escape arm consumed and discarded the
/// terminating character. When that character was the first byte of a
/// single-line PKCS#8 body, the candidate dropped 64 -> 63, below
/// `full_width`, and with one body line `qualifying >= 2` also failed.
#[test]
fn b10_3_an_unknown_escape_does_not_eat_the_first_body_byte() {
    for (name, key) in [("ed25519", ED25519_PKCS8), ("x25519", X25519_PKCS8)] {
        let body = key
            .lines()
            .find(|l| l.len() >= 60)
            .expect("single-line body");
        // The body must reach the DECODER, so it is on the banner's own
        // physical line with escaped breaks — and a backslash sits
        // immediately before the body's FIRST byte, so the character the
        // unknown-escape arm eats is that byte.
        let doc = format!("-----BEGIN PRIVATE KEY-----\\n\\{body}\\n-----END PRIVATE KEY-----");
        assert!(
            !leaks(&doc, &body[1..]),
            "{name}: an unknown escape ate the first body byte and the key shipped"
        );
        // And the untouched key still masks.
        assert!(!leaks(key, body), "{name}: plain PKCS#8 key leaked");
    }
}

/// BLOCKING 4, as behaviour: an unknown escape must NOT break the run.
/// Breaking is the UNSAFE direction here — `is_key_body_run` needs 40+
/// characters, so more breaks means shorter segments means fewer
/// qualifying lines means the block ships.
#[test]
fn b10_4_an_unknown_escape_keeps_the_run_intact() {
    let body = "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds";
    // Unknown escapes sprinkled INSIDE the payload must not fragment it.
    let peppered = format!("{}\\q{}", &body[..30], &body[30..]);
    let doc = format!("-----BEGIN PRIVATE KEY-----\n{peppered}\n-----END PRIVATE KEY-----");
    assert!(
        !leaks(&doc, &body[30..]),
        "an unknown escape inside the payload fragmented it and the key shipped"
    );
}

/// The transports that already worked must keep working.
#[test]
fn b10_5_the_established_transports_still_mask() {
    let frag = "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLds";
    let lf = RSA_2048.to_string();
    for (name, doc) in [
        ("raw-lf", lf.clone()),
        ("raw-crlf", lf.replace('\n', "\r\n")),
        ("json-lf", lf.replace('\n', "\\n")),
        ("json-crlf", lf.replace('\n', "\\r\\n")),
        ("json-tab", lf.replace('\n', "\\t")),
        ("percent-crlf", lf.replace('\n', "%0D%0A")),
    ] {
        assert!(!leaks(&doc, frag), "{name} regressed");
    }
}
