pub mod ast;
pub mod draw_list;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod renderer;
pub mod svg_emitter;

use draw_list::DrawList;
use eval::Evaluator;
use lexer::Lexer;
use parser::Parser;

pub fn compile_pvg(source: &str) -> Result<DrawList, String> {
    compile_pvg_at_time(source, 0.0)
}

pub fn compile_pvg_at_time(source: &str, time: f64) -> Result<DrawList, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize_all()?;
    let mut parser = Parser::new(tokens);
    let doc = parser.parse_document()?;
    let evaluator = Evaluator::new_with_time(time);
    evaluator.evaluate_document(&doc)
}

pub fn transpile_pvg_to_svg(source: &str, time: f64) -> Result<String, String> {
    let draw_list = compile_pvg_at_time(source, time)?;
    Ok(svg_emitter::emit_svg(&draw_list))
}