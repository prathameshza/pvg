pub mod ast;
pub mod draw_list;
pub mod eval;
pub mod lexer;
pub mod parser;

pub use ast::{Color, Document};
pub use draw_list::{DrawCmd, DrawList, DrawPathCommand, DrawStyle, Transform2D};
pub use eval::Evaluator;
pub use lexer::Lexer;
pub use parser::Parser;

pub fn parse_pvg(source: &str) -> Result<Document, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize_all()?;
    let mut parser = Parser::new(tokens);
    parser.parse_document()
}

pub fn compile_pvg(source: &str) -> Result<DrawList, String> {
    compile_pvg_at_time(source, 0.0)
}

pub fn compile_pvg_at_time(source: &str, time: f64) -> Result<DrawList, String> {
    let doc = parse_pvg(source)?;
    let evaluator = Evaluator::new_with_time(time);
    evaluator.evaluate_document(&doc)
}
