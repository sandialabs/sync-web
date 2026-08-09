use journal_sdk::JOURNAL;

fn main() {
    let expression = std::env::var("EXPRESSION").expect("EXPRESSION");
    println!("{}", JOURNAL.evaluate(&expression));
}
