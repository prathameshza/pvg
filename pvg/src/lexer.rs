use crate::ast::Color;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Indent,
    Dedent,
    Newline,
    Eof,

    // Literals
    Number(f64),
    String(String),
    Color(Color),
    Ident(String),

    // Keywords
    Pvg,
    Canvas,
    Background,
    Set,
    Def,
    Return,
    For,
    From,
    To,
    Step,
    While,
    If,
    Else,
    Seed,

    // Shapes
    Circle,
    Ellipse,
    Rectangle,
    Line,
    Polygon,
    Path,
    Group,

    // Properties
    Center,
    Radius,
    Pos,
    Size,
    Points,
    Fill,
    Stroke,
    Width,
    Opacity,
    Rot,
    Scale,

    // Path commands
    Start,
    Quad,
    Curve,
    Arc,
    Close,

    // Symbols & Operators
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Question,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Equal,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Not,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'a> {
    lines: Vec<&'a str>,
    current_line_idx: usize,
    indent_stack: Vec<usize>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let lines: Vec<&'a str> = source.lines().collect();
        Self {
            lines,
            current_line_idx: 0,
            indent_stack: vec![0],
        }
    }

    pub fn tokenize_all(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while self.current_line_idx < self.lines.len() {
            let raw_line = self.lines[self.current_line_idx];
            let line_num = self.current_line_idx + 1;
            self.current_line_idx += 1;

            let trimmed = raw_line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let spaces = raw_line.chars().take_while(|c| *c == ' ').count();
            if raw_line.chars().any(|c| c == '\t') {
                return Err(format!("Line {}: Tabs are forbidden. Use 2 spaces for indentation.", line_num));
            }

            let current_indent = *self.indent_stack.last().unwrap();
            if spaces > current_indent {
                self.indent_stack.push(spaces);
                tokens.push(Token {
                    kind: TokenKind::Indent,
                    line: line_num,
                    col: spaces + 1,
                });
            } else if spaces < current_indent {
                while let Some(&last) = self.indent_stack.last() {
                    if spaces < last {
                        self.indent_stack.pop();
                        tokens.push(Token {
                            kind: TokenKind::Dedent,
                            line: line_num,
                            col: spaces + 1,
                        });
                    } else if spaces == last {
                        break;
                    } else {
                        return Err(format!("Line {}: Inconsistent indentation level.", line_num));
                    }
                }
            }

            let content = &raw_line[spaces..];
            let line_tokens = self.tokenize_line(content, line_num, spaces + 1)?;
            tokens.extend(line_tokens);
            tokens.push(Token {
                kind: TokenKind::Newline,
                line: line_num,
                col: raw_line.len() + 1,
            });
        }

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            tokens.push(Token {
                kind: TokenKind::Dedent,
                line: self.lines.len().max(1),
                col: 1,
            });
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            line: self.lines.len().max(1),
            col: 1,
        });

        Ok(tokens)
    }

    fn tokenize_line(&self, text: &str, line_num: usize, col_offset: usize) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }

            let col = col_offset + i;

            if c == '#' {
                let mut hex_end = i + 1;
                while hex_end < len && chars[hex_end].is_ascii_hexdigit() {
                    hex_end += 1;
                }
                let hex_len = hex_end - (i + 1);
                if hex_len == 3 || hex_len == 6 || hex_len == 8 {
                    let is_delim = hex_end == len
                        || chars[hex_end].is_whitespace()
                        || chars[hex_end] == ']'
                        || chars[hex_end] == ')'
                        || chars[hex_end] == ','
                        || chars[hex_end] == ':';
                    if is_delim {
                        let hex_str: String = chars[i..hex_end].iter().collect();
                        if let Some(col_val) = Color::from_hex(&hex_str) {
                            tokens.push(Token {
                                kind: TokenKind::Color(col_val),
                                line: line_num,
                                col,
                            });
                            i = hex_end;
                            continue;
                        }
                    }
                }
                break;
            }

            if c == '[' { tokens.push(Token { kind: TokenKind::LBracket, line: line_num, col }); i += 1; continue; }
            if c == ']' { tokens.push(Token { kind: TokenKind::RBracket, line: line_num, col }); i += 1; continue; }
            if c == '(' { tokens.push(Token { kind: TokenKind::LParen, line: line_num, col }); i += 1; continue; }
            if c == ')' { tokens.push(Token { kind: TokenKind::RParen, line: line_num, col }); i += 1; continue; }
            if c == ',' { tokens.push(Token { kind: TokenKind::Comma, line: line_num, col }); i += 1; continue; }
            if c == '?' { tokens.push(Token { kind: TokenKind::Question, line: line_num, col }); i += 1; continue; }
            if c == ':' { tokens.push(Token { kind: TokenKind::Colon, line: line_num, col }); i += 1; continue; }
            if c == '^' { tokens.push(Token { kind: TokenKind::Caret, line: line_num, col }); i += 1; continue; }

            if c == '=' {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token { kind: TokenKind::EqualEqual, line: line_num, col });
                    i += 2;
                } else {
                    tokens.push(Token { kind: TokenKind::Equal, line: line_num, col });
                    i += 1;
                }
                continue;
            }

            if c == '!' {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token { kind: TokenKind::NotEqual, line: line_num, col });
                    i += 2;
                } else {
                    tokens.push(Token { kind: TokenKind::Not, line: line_num, col });
                    i += 1;
                }
                continue;
            }

            if c == '<' {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token { kind: TokenKind::LessEqual, line: line_num, col });
                    i += 2;
                } else {
                    tokens.push(Token { kind: TokenKind::Less, line: line_num, col });
                    i += 1;
                }
                continue;
            }

            if c == '>' {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token { kind: TokenKind::GreaterEqual, line: line_num, col });
                    i += 2;
                } else {
                    tokens.push(Token { kind: TokenKind::Greater, line: line_num, col });
                    i += 1;
                }
                continue;
            }

            if c == '&' && i + 1 < len && chars[i + 1] == '&' {
                tokens.push(Token { kind: TokenKind::And, line: line_num, col });
                i += 2;
                continue;
            }
            if c == '|' && i + 1 < len && chars[i + 1] == '|' {
                tokens.push(Token { kind: TokenKind::Or, line: line_num, col });
                i += 2;
                continue;
            }

            if c == '+' { tokens.push(Token { kind: TokenKind::Plus, line: line_num, col }); i += 1; continue; }
            if c == '-' { tokens.push(Token { kind: TokenKind::Minus, line: line_num, col }); i += 1; continue; }
            if c == '*' { tokens.push(Token { kind: TokenKind::Star, line: line_num, col }); i += 1; continue; }
            if c == '/' { tokens.push(Token { kind: TokenKind::Slash, line: line_num, col }); i += 1; continue; }
            if c == '%' { tokens.push(Token { kind: TokenKind::Percent, line: line_num, col }); i += 1; continue; }

            if c == '"' {
                i += 1;
                let mut s = String::new();
                let mut closed = false;
                while i < len {
                    if chars[i] == '\\' && i + 1 < len {
                        match chars[i + 1] {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            '\"' => s.push('\"'),
                            '\\' => s.push('\\'),
                            other => s.push(other),
                        }
                        i += 2;
                    } else if chars[i] == '"' {
                        closed = true;
                        i += 1;
                        break;
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                if !closed {
                    return Err(format!("Line {}: Unclosed string literal.", line_num));
                }
                tokens.push(Token { kind: TokenKind::String(s), line: line_num, col });
                continue;
            }

            if c.is_ascii_digit() || (c == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
                let start = i;
                let mut has_dot = false;
                while i < len && (chars[i].is_ascii_digit() || (!has_dot && chars[i] == '.')) {
                    if chars[i] == '.' {
                        has_dot = true;
                    }
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let mut val = num_str.parse::<f64>().map_err(|e| format!("Line {}: {}", line_num, e))?;

                if i + 3 <= len && &chars[i..i + 3] == &['d', 'e', 'g'] {
                    val = val.to_radians();
                    i += 3;
                } else if i + 3 <= len && &chars[i..i + 3] == &['r', 'a', 'd'] {
                    i += 3;
                }

                tokens.push(Token { kind: TokenKind::Number(val), line: line_num, col });
                continue;
            }

            if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                let kind = match ident.as_str() {
                    "PVG" | "CPSVG" => TokenKind::Pvg,
                    "canvas" => TokenKind::Canvas,
                    "background" => TokenKind::Background,
                    "set" => TokenKind::Set,
                    "def" => TokenKind::Def,
                    "return" => TokenKind::Return,
                    "for" => TokenKind::For,
                    "from" => TokenKind::From,
                    "to" => TokenKind::To,
                    "step" => TokenKind::Step,
                    "while" => TokenKind::While,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "seed" => TokenKind::Seed,
                    "circle" => TokenKind::Circle,
                    "ellipse" => TokenKind::Ellipse,
                    "rectangle" | "rect" => TokenKind::Rectangle,
                    "line" => TokenKind::Line,
                    "polygon" => TokenKind::Polygon,
                    "path" => TokenKind::Path,
                    "group" => TokenKind::Group,
                    "center" => TokenKind::Center,
                    "radius" => TokenKind::Radius,
                    "pos" => TokenKind::Pos,
                    "size" => TokenKind::Size,
                    "points" => TokenKind::Points,
                    "fill" => TokenKind::Fill,
                    "stroke" => TokenKind::Stroke,
                    "width" => TokenKind::Width,
                    "opacity" => TokenKind::Opacity,
                    "rot" => TokenKind::Rot,
                    "scale" => TokenKind::Scale,
                    "start" => TokenKind::Start,
                    "quad" => TokenKind::Quad,
                    "curve" => TokenKind::Curve,
                    "arc" => TokenKind::Arc,
                    "close" => TokenKind::Close,
                    "and" => TokenKind::And,
                    "or" => TokenKind::Or,
                    "not" => TokenKind::Not,
                    "black" => TokenKind::Color(Color::BLACK),
                    "white" => TokenKind::Color(Color::WHITE),
                    "red" => TokenKind::Color(Color::RED),
                    "green" => TokenKind::Color(Color::GREEN),
                    "blue" => TokenKind::Color(Color::BLUE),
                    "yellow" => TokenKind::Color(Color::YELLOW),
                    "cyan" => TokenKind::Color(Color::CYAN),
                    "magenta" => TokenKind::Color(Color::MAGENTA),
                    "none" | "transparent" => TokenKind::Color(Color::None),
                    "true" => TokenKind::Ident("true".into()),
                    "false" => TokenKind::Ident("false".into()),
                    _ => TokenKind::Ident(ident),
                };
                tokens.push(Token { kind, line: line_num, col });
                continue;
            }

            return Err(format!("Line {}, Col {}: Unexpected character '{}'.", line_num, col, c));
        }

        Ok(tokens)
    }
}