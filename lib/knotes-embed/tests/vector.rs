use knotes_embed::{DIMS, quantize, similarity};

#[test]
fn a_vector_is_cut_to_its_first_dims_normalized_and_kept_in_bytes() {
    let mut full = vec![0.0f32; 1024];
    (full[0], full[1], full[DIMS + 10]) = (3.0, 4.0, 100.0);
    let v = quantize(&full);
    assert_eq!(v.len(), DIMS);
    assert_eq!((v[0], v[1], v[2]), (76, 102, 0), "0.6 and 0.8 of 127; what lies past the cut counts for nothing");
    assert_eq!(quantize(&[0.0; 768]), [0; DIMS], "nothing to normalize stays nothing");
}

#[test]
fn the_nearer_vector_is_the_more_similar() {
    let unit = |i: usize, j: usize| {
        let mut v = vec![0.0f32; 768];
        (v[i], v[j]) = (1.0, 0.2);
        quantize(&v)
    };
    let (query, near, far) = (unit(0, 1), unit(0, 2), unit(5, 6));
    assert!(similarity(&query, &near) > similarity(&query, &far));
    assert!(similarity(&query, &query) > similarity(&query, &near));
}
