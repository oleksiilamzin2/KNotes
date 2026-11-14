use knotes_embed::Passage;

#[test]
fn queries_and_blocks_are_put_as_the_model_was_trained_on_them() {
    assert_eq!(Passage::Query("ремонт бойлера".into()).prompt(), "task: search result | query: ремонт бойлера");
    let block = Passage::Document { title: "Бойлер".into(), text: "Обслуговування › Анод міняти щороку.".into() };
    assert_eq!(block.prompt(), "title: Бойлер | text: Обслуговування › Анод міняти щороку.");
    assert_eq!(Passage::Document { title: String::new(), text: "x".into() }.prompt(), "title: none | text: x");
}
