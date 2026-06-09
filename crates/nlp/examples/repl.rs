use std::io::{self, BufRead, Write};
use nlp::{parse_query, NlpTokenizer};

fn main() {
    let tok = NlpTokenizer::new().expect("tokenizer init failed");

    println!("TreeScout NLP REPL — 日本語クエリを入力してください (Ctrl+C で終了)");
    println!("例: 先週保存したExcelファイル / 10MB以上のPDF / src/のTypeScript");
    println!("{}", "─".repeat(60));

    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Err(e) => { eprintln!("error: {e}"); break; }
            Ok(_) => {}
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        let parsed = parse_query(input);
        let everything_query = parsed.to_everything_query();

        let tokens = tok.tokenize(input).unwrap_or_default();
        let emb_tokens = tok.tokenize_for_embedding(input).unwrap_or_default();

        println!("  形態素:      {:?}", tokens);
        println!("  embedding用: {:?}", emb_tokens);
        println!("  拡張子:      {:?}", parsed.extensions);
        println!("  パス:        {:?}", parsed.path_filter);
        println!("  サイズ下限:  {:?}", parsed.min_size);
        println!("  サイズ上限:  {:?}", parsed.max_size);
        println!("  日付:        {:?}", parsed.date_modified);
        println!("  キーワード:  {:?}", parsed.keywords);
        println!("  ▶ Everything: {}", everything_query);
        println!("{}", "─".repeat(60));
    }
}
