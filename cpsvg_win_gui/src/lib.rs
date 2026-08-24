pub mod ast;
pub mod draw_list;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod png_rasterizer;
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

pub fn rasterize_pvg_to_png(source: &str, time: f64) -> Result<Vec<u8>, String> {
    png_rasterizer::rasterize_pvg_to_png(source, time, 1.0)
}

pub fn rasterize_pvg_to_png_scaled(source: &str, time: f64, scale: f32) -> Result<Vec<u8>, String> {
    png_rasterizer::rasterize_pvg_to_png(source, time, scale)
}

pub fn render_draw_list_to_png(draw_list: &DrawList) -> Result<Vec<u8>, String> {
    png_rasterizer::rasterize_draw_list_to_png(draw_list, 1.0)
}

pub fn render_draw_list_to_png_scaled(draw_list: &DrawList, scale: f32) -> Result<Vec<u8>, String> {
    png_rasterizer::rasterize_draw_list_to_png(draw_list, scale)
}