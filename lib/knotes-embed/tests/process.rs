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
