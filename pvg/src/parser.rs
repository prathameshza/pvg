use crate::ast::*;
use crate::error::PvgError;
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.peek_kind() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, PvgError> {
        let tok = self.peek().clone();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&kind) {
            Ok(self.advance())
        } else {
            Err(PvgError::parse(
                tok.line,
                tok.col,
                format!("Expected {:?}, found {:?}", kind, tok.kind),
            ))
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek_kind() == &TokenKind::Newline {
            self.advance();
        }
    }

    pub fn parse_document(&mut self) -> Result<Document, PvgError> {
        self.skip_newlines();

        // 1. Header: PVG 0.1
        self.expect(TokenKind::Pvg)?;
        let ver_tok = self.advance();
        let version = match ver_tok.kind {
            TokenKind::Number(v) => (v.floor() as u32, ((v - v.floor()) * 10.0).round() as u32),
            _ => {
                return Err(PvgError::parse_line(
                    ver_tok.line,
                    "Expected version number after PVG (e.g. 0.1)",
                ));
            }
        };
        self.skip_newlines();

        // 2. Canvas declaration
        self.expect(TokenKind::Canvas)?;
        let w_tok = self.advance();
        let h_tok = self.advance();
        let width = match w_tok.kind {
            TokenKind::Number(w) => w,
            _ => return Err(PvgError::parse_line(w_tok.line, "Expected canvas width number.")),
        };
        let height = match h_tok.kind {
            TokenKind::Number(h) => h,
            _ => return Err(PvgError::parse_line(h_tok.line, "Expected canvas height number.")),
        };

        let mut bg = None;
        if self.match_kind(&TokenKind::Newline) && self.match_kind(&TokenKind::Indent) {
            if self.match_kind(&TokenKind::Background) {
                let bg_tok = self.advance();
                bg = match bg_tok.kind {
                    TokenKind::Color(c) => Some(c),
                    _ => {
                        return Err(PvgError::parse_line(
                            bg_tok.line,
                            "Expected color for canvas background.",
                        ));
                    }
                };
            }
            self.skip_newlines();
            self.match_kind(&TokenKind::Dedent);
        }
        self.skip_newlines();

        // 3. Document statements
        let mut statements = Vec::new();
        while self.peek_kind() != &TokenKind::Eof {
            if self.peek_kind() == &TokenKind::Newline {
                self.advance();
                continue;
            }
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }

        Ok(Document {
            version,
            canvas: CanvasDecl {
                width,
                height,
                background: bg,
            },
            statements,
        })
    }

    fn parse_statement(&mut self) -> Result<Stmt, PvgError> {
        let peek_tok = self.peek().clone();
        match peek_tok.kind {
            TokenKind::Set => {
                self.advance();
                let name = match self.advance().kind {
                    TokenKind::Ident(s) => s,
                    t => {
                        return Err(PvgError::parse(
                            self.peek().line,
                            self.peek().col,
                            format!("Expected variable name after 'set', found {:?}", t),
                        ));
                    }
                };
                self.expect(TokenKind::Equal)?;
                let expr = self.parse_expression()?;
                Ok(Stmt::Set(name, expr))
            }
            TokenKind::Seed => {
                self.advance();
                let seed_val = match self.advance().kind {
                    TokenKind::Number(n) => n as u64,
                    _ => 0,
                };
                Ok(Stmt::Seed(seed_val))
            }
            TokenKind::Def => {
                self.advance();
                let name = match self.advance().kind {
                    TokenKind::Ident(s) => s,
                    _ => return Err(PvgError::parse_line(self.peek().line, "Expected function name.")),
                };
                self.expect(TokenKind::LParen)?;
                let mut params = Vec::new();
                if self.peek_kind() != &TokenKind::RParen {
                    loop {
                        if let TokenKind::Ident(p) = self.advance().kind {
                            params.push(p);
                        }
                        if self.peek_kind() == &TokenKind::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RParen)?;
                self.skip_newlines();
                let body = self.parse_block()?;
                Ok(Stmt::Def(FunctionDef { name, params, body }))
            }
            TokenKind::For => {
                self.advance();
                let var = match self.advance().kind {
                    TokenKind::Ident(s) => s,
                    _ => return Err(PvgError::parse_line(self.peek().line, "Expected loop variable name.")),
                };
                self.expect(TokenKind::From)?;
                let from_expr = self.parse_expression()?;
                self.expect(TokenKind::To)?;
                let to_expr = self.parse_expression()?;
                let mut step = None;
                if self.match_kind(&TokenKind::Step) {
                    step = Some(self.parse_expression()?);
                }
                self.skip_newlines();
                let body = self.parse_block()?;
                Ok(Stmt::For {
                    var,
                    from: from_expr,
                    to: to_expr,
                    step,
                    body,
                })
            }
            TokenKind::While => {
                self.advance();
                let cond = self.parse_expression()?;
                self.skip_newlines();
                let body = self.parse_block()?;
                Ok(Stmt::While { cond, body })
            }
            TokenKind::If => {
                self.advance();
                let cond = self.parse_expression()?;
                self.skip_newlines();
                let then_body = self.parse_block()?;
                let mut else_body = Vec::new();
                self.skip_newlines();
                if self.match_kind(&TokenKind::Else) {
                    if self.peek_kind() == &TokenKind::If {
                        else_body.push(self.parse_statement()?);
                    } else {
                        self.skip_newlines();
                        else_body = self.parse_block()?;
                    }
                }
                Ok(Stmt::If { cond, then_body, else_body })
            }
            TokenKind::Return => {
                self.advance();
                let expr = self.parse_expression()?;
                Ok(Stmt::Return(expr))
            }
            TokenKind::Circle => {
                self.advance();
                self.skip_newlines();
                self.parse_circle()
            }
            TokenKind::Ellipse => {
                self.advance();
                self.skip_newlines();
                self.parse_ellipse()
            }
            TokenKind::Rectangle => {
                self.advance();
                self.skip_newlines();
                self.parse_rectangle()
            }
            TokenKind::Line => {
                self.advance();
                self.skip_newlines();
                self.parse_line()
            }
            TokenKind::Polygon => {
                self.advance();
                self.skip_newlines();
                self.parse_polygon()
            }
            TokenKind::Path => {
                self.advance();
                self.skip_newlines();
                self.parse_path()
            }
            TokenKind::Text => {
                self.advance();
                self.skip_newlines();
                self.parse_text()
            }
            TokenKind::Group => {
                self.advance();
                self.skip_newlines();
                self.parse_group()
            }
            TokenKind::Ident(ref name) => {
                let func_name = name.clone();
                self.advance();
                if self.match_kind(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    if self.peek_kind() != &TokenKind::RParen {
                        loop {
                            args.push(self.parse_expression()?);
                            if self.peek_kind() == &TokenKind::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Stmt::Call(func_name, args))
                } else {
                    Err(PvgError::parse_line(
                        self.peek().line,
                        "Unexpected identifier in statement position.",
                    ))
                }
            }
            other => Err(PvgError::parse(
                self.peek().line,
                self.peek().col,
                format!("Unexpected statement token {:?}", other),
            )),
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut stmts = Vec::new();
        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            if self.peek_kind() == &TokenKind::Newline {
                self.advance();
                continue;
            }
            stmts.push(self.parse_statement()?);
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;
        Ok(stmts)
    }

    fn parse_circle(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut center = None;
        let mut radius = None;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Center => { self.advance(); center = Some(self.parse_expression()?); }
                TokenKind::Radius => { self.advance(); radius = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid circle property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        let center = center.ok_or_else(|| PvgError::parse_line(self.peek().line, "Circle requires 'center [x, y]'"))?;
        let radius = radius.ok_or_else(|| PvgError::parse_line(self.peek().line, "Circle requires 'radius r'"))?;
        Ok(Stmt::Circle(CircleNode { center, radius, fill, stroke, width, opacity }))
    }

    fn parse_ellipse(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut center = None;
        let mut radius = None;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Center => { self.advance(); center = Some(self.parse_expression()?); }
                TokenKind::Radius => { self.advance(); radius = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid ellipse property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        let center = center.ok_or_else(|| PvgError::parse_line(self.peek().line, "Ellipse requires 'center [x, y]'"))?;
        let radius = radius.ok_or_else(|| PvgError::parse_line(self.peek().line, "Ellipse requires 'radius [rx, ry]'"))?;
        Ok(Stmt::Ellipse(EllipseNode { center, radius, fill, stroke, width, opacity }))
    }

    fn parse_rectangle(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut pos = None;
        let mut size = None;
        let mut radius = None;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => { self.advance(); pos = Some(self.parse_expression()?); }
                TokenKind::Size => { self.advance(); size = Some(self.parse_expression()?); }
                TokenKind::Radius => { self.advance(); radius = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid rectangle property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        let pos = pos.ok_or_else(|| PvgError::parse_line(self.peek().line, "Rectangle requires 'pos [x, y]'"))?;
        let size = size.ok_or_else(|| PvgError::parse_line(self.peek().line, "Rectangle requires 'size [w, h]'"))?;
        Ok(Stmt::Rectangle(RectNode { pos, size, radius, fill, stroke, width, opacity }))
    }

    fn parse_line(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut from = None;
        let mut to = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::From => { self.advance(); from = Some(self.parse_expression()?); }
                TokenKind::To => { self.advance(); to = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid line property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        let from = from.ok_or_else(|| PvgError::parse_line(self.peek().line, "Line requires 'from [x, y]'"))?;
        let to = to.ok_or_else(|| PvgError::parse_line(self.peek().line, "Line requires 'to [x, y]'"))?;
        Ok(Stmt::Line(LineNode { from, to, stroke, width, opacity }))
    }

    fn parse_polygon(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut points = Vec::new();
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Points => {
                    self.advance();
                    while self.peek_kind() == &TokenKind::LBracket {
                        points.push(self.parse_expression()?);
                    }
                }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid polygon property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        Ok(Stmt::Polygon(PolygonNode { points, fill, stroke, width, opacity }))
    }

    fn parse_path(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut commands = Vec::new();

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Set => {
                    self.advance();
                    let name = match self.advance().kind {
                        TokenKind::Ident(s) => s,
                        t => {
                            return Err(PvgError::parse(
                                self.peek().line,
                                self.peek().col,
                                format!("Expected variable name after 'set', found {:?}", t),
                            ));
                        }
                    };
                    self.expect(TokenKind::Equal)?;
                    let expr = self.parse_expression()?;
                    commands.push(PathCommand::Set(name, expr));
                }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Start => { self.advance(); commands.push(PathCommand::Start(self.parse_expression()?)); }
                TokenKind::Line => { self.advance(); commands.push(PathCommand::Line(self.parse_expression()?)); }
                TokenKind::Quad => {
                    self.advance();
                    let cp = self.parse_expression()?;
                    let ep = self.parse_expression()?;
                    commands.push(PathCommand::Quad(cp, ep));
                }
                TokenKind::Curve => {
                    self.advance();
                    let c1 = self.parse_expression()?;
                    let c2 = self.parse_expression()?;
                    let ep = self.parse_expression()?;
                    commands.push(PathCommand::Curve(c1, c2, ep));
                }
                TokenKind::Arc => {
                    self.advance();
                    let center = self.parse_expression()?;
                    let radius = self.parse_expression()?;
                    let start_angle = self.parse_expression()?;
                    let end_angle = self.parse_expression()?;
                    commands.push(PathCommand::Arc { center, radius, start_angle, end_angle });
                }
                TokenKind::Close => { self.advance(); commands.push(PathCommand::Close); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid path property/command {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        Ok(Stmt::Path(PathNode { fill, stroke, width, opacity, commands }))
    }

    fn parse_text(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut pos = None;
        let mut content = None;
        let mut size = None;
        let mut font = None;
        let mut align = None;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => { self.advance(); pos = Some(self.parse_expression()?); }
                TokenKind::Content | TokenKind::Text => { self.advance(); content = Some(self.parse_expression()?); }
                TokenKind::Size => { self.advance(); size = Some(self.parse_expression()?); }
                TokenKind::Font => { self.advance(); font = Some(self.parse_expression()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid text property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        let pos = pos.ok_or_else(|| PvgError::parse_line(self.peek().line, "Text requires 'pos [x, y]'"))?;
        let content = content.ok_or_else(|| PvgError::parse_line(self.peek().line, "Text requires 'content <expr>' or 'text <expr>'"))?;
        Ok(Stmt::Text(TextNode { pos, content, size, font, align, fill, stroke, width, opacity }))
    }

    fn parse_group(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut pos = None;
        let mut rot = None;
        let mut scale = None;
        let mut opacity = None;
        let mut fill = None;
        let mut stroke = None;
        let mut body = Vec::new();

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => { self.advance(); pos = Some(self.parse_expression()?); }
                TokenKind::Rot => { self.advance(); rot = Some(self.parse_expression()?); }
                TokenKind::Scale => { self.advance(); scale = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_expression()?); }
                TokenKind::Newline => { self.advance(); }
                _ => { body.push(self.parse_statement()?); }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        Ok(Stmt::Group(GroupNode { pos, rot, scale, opacity, fill, stroke, body }))
    }

    pub fn parse_expression(&mut self) -> Result<Expr, PvgError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, PvgError> {
        let cond = self.parse_logical_or()?;
        if self.match_kind(&TokenKind::Question) {
            let true_branch = self.parse_expression()?;
            self.expect(TokenKind::Colon)?;
            let false_branch = self.parse_expression()?;
            Ok(Expr::Ternary(
                Box::new(cond),
                Box::new(true_branch),
                Box::new(false_branch),
            ))
        } else {
            Ok(cond)
        }
    }

    fn parse_logical_or(&mut self) -> Result<Expr, PvgError> {
        let mut left = self.parse_logical_and()?;
        while self.match_kind(&TokenKind::Or) {
            let right = self.parse_logical_and()?;
            left = Expr::Binary(Box::new(left), BinaryOp::Or, Box::new(right));
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, PvgError> {
        let mut left = self.parse_equality()?;
        while self.match_kind(&TokenKind::And) {
            let right = self.parse_equality()?;
            left = Expr::Binary(Box::new(left), BinaryOp::And, Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, PvgError> {
        let mut left = self.parse_comparison()?;
        while let TokenKind::EqualEqual | TokenKind::NotEqual = self.peek_kind() {
            let op = match self.advance().kind {
                TokenKind::EqualEqual => BinaryOp::Eq,
                TokenKind::NotEqual => BinaryOp::Ne,
                _ => unreachable!(),
            };
            let right = self.parse_comparison()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, PvgError> {
        let mut left = self.parse_additive()?;
        while let TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual = self.peek_kind() {
            let op = match self.advance().kind {
                TokenKind::Less => BinaryOp::Lt,
                TokenKind::LessEqual => BinaryOp::Le,
                TokenKind::Greater => BinaryOp::Gt,
                TokenKind::GreaterEqual => BinaryOp::Ge,
                _ => unreachable!(),
            };
            let right = self.parse_additive()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, PvgError> {
        let mut left = self.parse_multiplicative()?;
        while let TokenKind::Plus | TokenKind::Minus = self.peek_kind() {
            let op = match self.advance().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_multiplicative()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, PvgError> {
        let mut left = self.parse_power()?;
        while let TokenKind::Star | TokenKind::Slash | TokenKind::Percent = self.peek_kind() {
            let op = match self.advance().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            let right = self.parse_power()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, PvgError> {
        let left = self.parse_unary()?;
        if self.match_kind(&TokenKind::Caret) {
            let right = self.parse_power()?;
            Ok(Expr::Binary(Box::new(left), BinaryOp::Pow, Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, PvgError> {
        if self.match_kind(&TokenKind::Minus) {
            let expr = self.parse_unary()?;
            Ok(Expr::Unary(UnaryOp::Neg, Box::new(expr)))
        } else if self.match_kind(&TokenKind::Not) {
            let expr = self.parse_unary()?;
            Ok(Expr::Unary(UnaryOp::Not, Box::new(expr)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, PvgError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::Number(n) => Ok(Expr::Number(n)),
            TokenKind::String(s) => Ok(Expr::String(s)),
            TokenKind::Color(c) => Ok(Expr::Color(c)),
            TokenKind::LBracket => {
                let x = self.parse_expression()?;
                self.expect(TokenKind::Comma)?;
                let y = self.parse_expression()?;
                self.expect(TokenKind::RBracket)?;
                Ok(Expr::Vec2(Box::new(x), Box::new(y)))
            }
            TokenKind::LParen => {
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Ident(s) => {
                if s == "true" { return Ok(Expr::Bool(true)); }
                if s == "false" { return Ok(Expr::Bool(false)); }

                if self.match_kind(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    if self.peek_kind() != &TokenKind::RParen {
                        loop {
                            args.push(self.parse_expression()?);
                            if self.peek_kind() == &TokenKind::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Call(s, args))
                } else {
                    Ok(Expr::Ident(s))
                }
            }
            other => Err(PvgError::parse(
                tok.line,
                tok.col,
                format!("Unexpected token in expression: {:?}", other),
            )),
        }
    }
}