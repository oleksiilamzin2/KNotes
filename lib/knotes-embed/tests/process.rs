use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use knotes_embed::{DIMS, EmbedError, Embedder, Passage, ProcessEmbedder, protocol};

/// A stand-in for the helper: answers one request with the reply in the file it is given as
/// its model, then exits, as the helper does once it has been idle.
fn helper(dir: &Path, reply: &Result<Vec<knotes_embed::Vector>, EmbedError>) -> (PathBuf, PathBuf) {
    let (script, answer) = (dir.join("helper.sh"), dir.join("reply.json"));
    std::fs::write(&answer, protocol::response(reply) + "\n").unwrap();
    std::fs::write(&script, "#!/bin/sh\nread line\ncat \"$1\"\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, answer)
}

#[test]
fn vectors_come_from_the_helper_started_again_once_it_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (script, answer) = helper(dir.path(), &Ok(vec![[3; DIMS]]));
    let embedder = ProcessEmbedder::new(&script, &answer);
    assert_eq!(embedder.embed(&[Passage::Query("бойлер".into())]), Ok(vec![[3; DIMS]]));
    assert_eq!(embedder.embed(&[Passage::Query("анод".into())]), Ok(vec![[3; DIMS]]), "the helper had exited: started again");
    assert_eq!(embedder.embed(&[]), Ok(vec![]), "nothing to ask, nothing started");
}

#[test]
fn warming_starts_the_helper_ahead_of_a_request_and_again_once_it_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (script, answer) = helper(dir.path(), &Ok(vec![[3; DIMS]]));
    // This helper marks that it has started, as the real one starts loading its model.
    std::fs::write(&script, "#!/bin/sh\ntouch \"$1.started\"\nread line\ncat \"$1\"\n").unwrap();
    let started = answer.with_extension("json.started");
    let wait_started = || {
        let start = std::time::Instant::now();
        while !started.exists() {
            assert!(start.elapsed() < std::time::Duration::from_secs(5), "not started");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    let embedder = ProcessEmbedder::new(&script, &answer);
    embedder.warm();
    wait_started();
    assert_eq!(embedder.embed(&[Passage::Query("бойлер".into())]), Ok(vec![[3; DIMS]]), "the warmed helper answers");
    std::fs::remove_file(&started).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    embedder.warm();
    wait_started();
}

#[test]
fn a_missing_helper_or_a_wrong_answer_says_so() {
    let missing = ProcessEmbedder::new("/nonexistent/knotes-embed", "/nonexistent/model.gguf");
    assert!(matches!(missing.embed(&[Passage::Query("x".into())]), Err(EmbedError::Unavailable(_))));
    let dir = tempfile::tempdir().unwrap();
    let (script, answer) = helper(dir.path(), &Ok(vec![[1; DIMS]]));
    let two = [Passage::Query("a".into()), Passage::Query("b".into())];
    assert!(matches!(ProcessEmbedder::new(&script, &answer).embed(&two), Err(EmbedError::Failed(_))), "one vector for two passages");
    let (script, answer) = helper(dir.path(), &Err(EmbedError::Failed("no memory".into())));
    assert_eq!(ProcessEmbedder::new(&script, &answer).embed(&two[..1]), Err(EmbedError::Failed("no memory".into())));
}

#[test]
fn a_helper_that_hangs_is_stopped_at_the_deadline_and_the_next_request_starts_another() {
    let dir = tempfile::tempdir().unwrap();
    let (script, answer) = helper(dir.path(), &Ok(vec![[3; DIMS]]));
    // The first one started reads the request and says nothing (its pid kept); those after it
    // answer.
    let pid = dir.path().join("hung.pid");
    std::fs::write(&script, format!("#!/bin/sh\nif [ -e '{0}' ]; then read line; cat \"$1\"; else echo $$ > '{0}'; read line; sleep 30; fi\n", pid.display())).unwrap();
    let embedder = ProcessEmbedder::with_deadline(&script, &answer, std::time::Duration::from_millis(300));
    let start = std::time::Instant::now();
    let hung = embedder.embed(&[Passage::Query("бойлер".into())]);
    assert!(matches!(&hung, Err(EmbedError::Failed(why)) if why.contains("did not answer")), "{hung:?}");
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "not asked again: {:?}", start.elapsed());
    let pid = std::fs::read_to_string(&pid).unwrap();
    let alive = std::process::Command::new("kill").args(["-0", pid.trim()]).status().unwrap().success();
    assert!(!alive, "the hung helper was stopped");
    assert_eq!(embedder.embed(&[Passage::Query("анод".into())]), Ok(vec![[3; DIMS]]), "another started");
}

#[test]
fn a_helper_that_dies_says_how_it_ended() {
    let dir = tempfile::tempdir().unwrap();
    let (script, answer) = helper(dir.path(), &Ok(vec![[3; DIMS]]));
    std::fs::write(&script, "#!/bin/sh\nexit 3\n").unwrap();
    let ended = ProcessEmbedder::new(&script, &answer).embed(&[Passage::Query("x".into())]);
    assert!(matches!(&ended, Err(EmbedError::Failed(why)) if why.contains("stopped answering") && why.contains('3')), "{ended:?}");
}
