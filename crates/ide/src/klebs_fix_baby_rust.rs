use syntax::{SourceFile,TextRange};
use text_edit::TextEdit;

#[derive(Debug)]
pub struct KlebsFixBabyRustConfig {

}

fn test_write() -> Result<(),std::io::Error> {

    use std::io::Write;

    let path = "/Users/kleb/bethesda/work/repo/translator/testing123.txt";

    let mut output = std::fs::File::create(path)?;

    write!(output, "Rust\n💖\nFun")?;

    Ok(())
}

// Feature: Klebs Fix Baby Rust
//
// chomper integrations!
//
pub(crate) fn klebs_fix_baby_rust(
    config: &KlebsFixBabyRustConfig,
    file: &SourceFile,
    range: TextRange,
) -> TextEdit {

    tracing::info!("klebs_fix_baby_rust");
    tracing::info!("config: {:?}", config);
    tracing::info!("file: {:?}",   file);
    tracing::info!("range: {:?}",  range);

    test_write().unwrap();

    panic!("muahahha");
    todo!();

    /*
    let range = if range.is_empty() {
        let syntax = file.syntax();
        let text = syntax.text().slice(range.start()..);
        let pos = match text.find_char('\n') {
            None => return TextEdit::builder().finish(),
            Some(pos) => pos,
        };
        TextRange::at(range.start() + pos, TextSize::of('\n'))
    } else {
        range
    };

    let mut edit = TextEdit::builder();

    match file.syntax().covering_element(range) {
        NodeOrToken::Node(node) => {
            for token in node.descendants_with_tokens().filter_map(|it| it.into_token()) {
                remove_newlines(config, &mut edit, &token, range)
            }
        }
        NodeOrToken::Token(token) => remove_newlines(config, &mut edit, &token, range),
    };

    edit.finish()
    */
}
