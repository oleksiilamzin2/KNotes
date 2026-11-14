use knotes_embed::{DIMS, EmbedError, Passage, protocol};

#[test]
fn a_batch_goes_to_the_helper_and_its_vectors_come_back_a_line_each_way() {
    let passages = vec![Passage::Query("бойлер".into()), Passage::Document { title: "Дім".into(), text: "Анод".into() }];
    let line = protocol::request(&passages);
    assert!(!line.contains('\n'), "one line");
    assert_eq!(protocol::read_request(&line), Ok(passages));

    let mut vector = [0i8; DIMS];
    (vector[0], vector[255]) = (-127, 127);
    let line = protocol::response(&Ok(vec![vector, [1; DIMS]]));
    assert!(!line.contains('\n'), "one line");
    assert_eq!(protocol::read_response(&line), Ok(vec![vector, [1; DIMS]]));

    let failed = protocol::response(&Err(EmbedError::Failed("out of memory".into())));
    assert_eq!(protocol::read_response(&failed), Err(EmbedError::Failed("out of memory".into())));
    assert!(matches!(protocol::read_response("not json"), Err(EmbedError::Failed(_))));
    assert!(matches!(protocol::read_request("{}"), Err(EmbedError::Failed(_))));
}
