use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Coordinator(Child);

impl Drop for Coordinator {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn await_file(path: &Path, coordinator: &mut Coordinator) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.is_file() {
        assert!(coordinator.0.try_wait().unwrap().is_none(), "daemon exited");
        assert!(Instant::now() < deadline, "missing {}", path.display());
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn coordinator_ignores_staged_requests_until_atomic_publication() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("helm-coordinator-publication-{nonce}"));
    let state = root.join("ipc");
    let requests = state.join("requests");
    let responses = state.join("responses");
    fs::create_dir_all(&requests).unwrap();
    fs::create_dir_all(root.join("home")).unwrap();
    // Match write_json_file's staging name, including a fully written request.
    // A later ping is a barrier: sorted directory processing has seen these files.
    let staged = requests.join("000-complete.json.tmp-123-0");
    let partial = requests.join("001-partial.json.tmp-123-1");

    let mut coordinator = Coordinator(
        Command::new(env!("CARGO_BIN_EXE_helm"))
            .env_clear()
            .env("HOME", root.join("home"))
            .env("HELM_DB_PATH", root.join("helm.db"))
            .env("HELM_ACCEPT_LICENSE", "1")
            .env("HELM_ACCEPT_DEFAULTS", "1")
            .env("PATH", "/usr/bin:/bin")
            .args(["__coordinator__", "serve", "--state-dir"])
            .arg(&state)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    await_file(&state.join("ready.json"), &mut coordinator);
    fs::write(&staged, br#"{"kind":"ping"}"#).unwrap();
    fs::write(&partial, br#"{"kind":"#).unwrap();
    let barrier = root.join("barrier-staging");
    fs::write(&barrier, br#"{"kind":"ping"}"#).unwrap();
    fs::rename(barrier, requests.join("999-barrier.json")).unwrap();
    await_file(&responses.join("999-barrier.json"), &mut coordinator);
    assert!(staged.is_file(), "unpublished request was consumed");
    assert!(partial.is_file(), "partial request was consumed");
    assert_eq!(fs::read_dir(&responses).unwrap().count(), 1);

    fs::rename(&staged, requests.join("000-complete.json")).unwrap();
    let published_response = responses.join("000-complete.json");
    await_file(&published_response, &mut coordinator);
    let payload: serde_json::Value =
        serde_json::from_slice(&fs::read(published_response).unwrap()).unwrap();
    assert_eq!(payload["ok"], true);
    assert_eq!(fs::read_dir(&responses).unwrap().count(), 2);
    assert!(partial.is_file());
}
