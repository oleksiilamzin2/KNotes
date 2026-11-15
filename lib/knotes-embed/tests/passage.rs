use knotes_embed::Passage;

#[test]
fn queries_and_blocks_are_put_as_the_model_was_trained_on_them() {
    assert_eq!(Passage::Query("ремонт бойлера".into()).prompt(), "task: search result | query: ремонт бойлера");
    let block = Passage::Document { title: "Бойлер".into(), text: "Обслуговування › Анод міняти щороку.".into() };
    assert_eq!(block.prompt(), "title: Бойлер | text: Обслуговування › Анод міняти щороку.");
    assert_eq!(Passage::Document { title: String::new(), text: "x".into() }.prompt(), "title: none | text: x");
}

#[test]
fn e5_takes_an_instruction_before_a_query_and_a_document_as_it_is() {
    use knotes_embed::PromptStyle;
    assert!(Passage::Query("ремонт бойлера".into()).prompt_in(PromptStyle::E5Instruct).starts_with("Instruct: "));
    assert!(Passage::Query("ремонт бойлера".into()).prompt_in(PromptStyle::E5Instruct).ends_with("\nQuery: ремонт бойлера"));
    assert_eq!(Passage::Document { title: "Бойлер".into(), text: "Анод щороку.".into() }.prompt_in(PromptStyle::E5Instruct), "Бойлер\nАнод щороку.");
    assert_eq!(Passage::Query("x".into()).prompt_in(PromptStyle::EmbeddingGemma), Passage::Query("x".into()).prompt(), "EmbeddingGemma 2 takes the first one's prompts");
}
