//! A small, opt-in timing sample. It is not a representative benchmark suite.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "manual performance measurement"]
fn transfer_loop_sample() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("bfcompile-perf-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("sample.bf");
    std::fs::write(&source, b"-[>-[>-[->+<]<-]<-]".repeat(10)).unwrap();
    for opt in ["-O0", "-O2"] {
        let binary = dir.join(if cfg!(windows) {
            format!("sample-{opt}.exe")
        } else {
            format!("sample-{opt}")
        });
        let result = Command::new(env!("CARGO_BIN_EXE_bfcompile"))
            .arg(opt)
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let start = Instant::now();
        let mut child = Command::new(binary)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() >= Duration::from_secs(30) {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("benchmark program timed out");
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(status.success());
        println!("{opt}: {:?}", start.elapsed());
    }
    let _ = std::fs::remove_dir_all(dir);
}
