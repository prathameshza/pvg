//! # ⚡ PVG — Procedural Vector Graphics Core Engine
//!
//! **PVG** is a deterministic, human-readable 2D vector graphics and procedural scene description language.
//! It combines the declarative clarity of vector graphics with native loops, typography, trigonometry,
//! and microsecond CPU evaluation.
//!
//! ## Key Features
//! - 🦀 **Zero GPU Dependency**: Evaluates pure vector geometry directly on the CPU.
//! - ⏱️ **Microsecond CPU Evaluation**: Evaluates animated scenes in `< 40 µs` per frame.
//! - 🪶 **Sub-50 KB Memory Footprint**: Evaluates flat contiguous 2D draw lists with zero DOM overhead.
//! - 🔄 **AST Caching**: Parse once into an AST, re-evaluate per frame across timeline ticks with no re-parsing.
//! - 🌐 **Built-in Standalone SVG Emitter**: Direct serialization into W3C SVG XML (static & SMIL animated).
//!
//! ## Quickstart Example
//!
//! ```rust
//! use pvg::{compile, compile_at_time, Color, DrawCmd};
//!
//! let source = r#"
//! PVG 0.1
//! canvas 400 400
//!   background #000000
//!
//! circle
//!   center [200, 200]
//!   radius 50 + 20 * sin(time * 3.0)
//!   fill #00ffcc
//! "#;
//!
//! // 1. Compile at time t = 0.0s
//! let draw_list = compile(source).expect("Compilation failed");
//! assert_eq!(draw_list.canvas_width, 400.0);
//! assert_eq!(draw_list.canvas_height, 400.0);
//! assert_eq!(draw_list.items.len(), 1);
//!
//! // 2. Serialize directly to W3C SVG
//! let svg_xml = draw_list.to_svg();
//! assert!(svg_xml.contains("<svg"));
//! assert!(svg_xml.contains("<circle"));
//!
//! // 3. Animate at time t = 1.5s
//! let animated_list = compile_at_time(source, 1.5).expect("Evaluation failed");
//! assert_eq!(animated_list.items.len(), 1);
//! ```
//!
//! ## Two-Phase Compilation (AST Caching Optimization)
//!
//! For real-time 60 FPS animation loops, avoid parsing the source string repeatedly.
//! Parse the AST document once and evaluate it on each frame tick:
//!
//! ```rust
//! use pvg::{parse, Evaluator};
//!
//! let source = "PVG 0.1\ncanvas 200 200\ncircle\n  center [100, 100]\n  radius 40\n";
//! let doc = parse(source).expect("Failed to parse document");
//!
//! // Re-evaluate the cached AST with updated timeline timestamps
//! for frame in 0..60 {
//!     let time = frame as f64 / 60.0;
//!     let evaluator = Evaluator::new_with_time(time);
//!     let draw_list = evaluator.evaluate_document(&doc).expect("Evaluation failed");
//!     assert_eq!(draw_list.items.len(), 1);
//! }
//! ```

pub mod ast;
pub mod draw_list;
pub mod error;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod svg;

// Re-export core types
pub use ast::{
    CanvasDecl, CircleNode, Color, Document, EllipseNode, Expr, FunctionDef, GroupNode, LineNode,
    PathCommand, PathNode, PolygonNode, RectNode, Stmt, TextNode, UnaryOp, BinaryOp,
};
pub use draw_list::{
    DrawCmd, DrawList, DrawPathCommand, DrawStyle, TextAlign, Transform2D,
};
pub use error::{PvgError, PvgErrorKind};
pub use eval::{Evaluator, Value};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::Parser;
pub use svg::{emit_animated_svg, emit_draw_commands, emit_svg, escape_xml, format_svg_attributes};

/// Parses a PVG source string into an Abstract Syntax Tree ([`Document`]).
///
/// # Errors
/// Returns [`PvgError`] if lexical or syntax errors occur.
pub fn parse(source: &str) -> Result<Document, PvgError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize_all()?;
    let mut parser = Parser::new(tokens);
    parser.parse_document()
}

/// Compiles a PVG source string into a flat 2D [`DrawList`] at `time = 0.0`.
///
/// # Errors
/// Returns [`PvgError`] if parsing or procedural evaluation fails.
pub fn compile(source: &str) -> Result<DrawList, PvgError> {
    compile_at_time(source, 0.0)
}

/// Compiles a PVG source string into a flat 2D [`DrawList`] at a specific timeline timestamp.
///
/// # Errors
/// Returns [`PvgError`] if parsing or procedural evaluation fails.
pub fn compile_at_time(source: &str, time: f64) -> Result<DrawList, PvgError> {
    let doc = parse(source)?;
    let evaluator = Evaluator::new_with_time(time);
    evaluator.evaluate_document(&doc)
}

/// Transpiles a PVG source string directly into a W3C SVG XML string at `time = 0.0`.
pub fn to_svg(source: &str) -> Result<String, PvgError> {
    to_svg_at_time(source, 0.0)
}

/// Transpiles a PVG source string directly into a W3C SVG XML string at a specific timeline timestamp.
pub fn to_svg_at_time(source: &str, time: f64) -> Result<String, PvgError> {
    let draw_list = compile_at_time(source, time)?;
    Ok(draw_list.to_svg())
}

// Backwards-compatible aliases
/// Alias for [`parse`] (backwards compatibility).
#[inline]
pub fn parse_pvg(source: &str) -> Result<Document, PvgError> {
    parse(source)
}

/// Alias for [`compile`] (backwards compatibility).
#[inline]
pub fn compile_pvg(source: &str) -> Result<DrawList, PvgError> {
    compile(source)
}

/// Alias for [`compile_at_time`] (backwards compatibility).
#[inline]
pub fn compile_pvg_at_time(source: &str, time: f64) -> Result<DrawList, PvgError> {
    compile_at_time(source, time)
}