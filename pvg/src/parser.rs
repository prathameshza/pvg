use crate::ast::{PixelFilter, *};
use crate::error::PvgError;
use crate::lexer::{Token, TokenKind};

/// Names for the post-0.2 "soft" keywords (`row`, `data`, `filter`, `snap`,
/// `palette`, `param`, `pattern`, `sprite`, `spline`).
///
/// These words open new syntax in statement/property position, but pre-0.3
/// documents (and presets like `grid.pvg`) use some of them as ordinary
/// variable names (`for row from 0 to 7`). Wherever an *identifier* is
/// expected, the soft keywords are accepted as names; their special meaning
/// applies only where their syntax applies.
fn soft_ident(kind: &TokenKind) -> Option<String> {
    match kind {
        TokenKind::Ident(s) => Some(s.clone()),
        TokenKind::Row => Some("row".into()),
        TokenKind::Data => Some("data".into()),
        TokenKind::Filter => Some("filter".into()),
        TokenKind::Snap => Some("snap".into()),
        TokenKind::Palette => Some("palette".into()),
        TokenKind::Param => Some("param".into()),
        TokenKind::Pattern => Some("pattern".into()),
        TokenKind::Sprite => Some("sprite".into()),
        TokenKind::Spline => Some("spline".into()),
        _ => None,
    }
}

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
        let mut snap = 0.0;
        let mut pixel_filter = PixelFilter::Linear;
        if self.match_kind(&TokenKind::Newline) && self.match_kind(&TokenKind::Indent) {
            loop {
                self.skip_newlines();
                if self.peek_kind() == &TokenKind::Dedent || self.peek_kind() == &TokenKind::Eof {
                    break;
                }
                match self.peek_kind() {
                    TokenKind::Background => {
                        self.advance();
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
                    TokenKind::Snap => {
                        self.advance();
                        let s_tok = self.advance();
                        snap = match s_tok.kind {
                            TokenKind::Number(v) => v.max(0.0),
                            _ => {
                                return Err(PvgError::parse_line(
                                    s_tok.line,
                                    "Expected number for canvas snap grid.",
                                ));
                            }
                        };
                    }
                    TokenKind::Filter => {
                        self.advance();
                        let f_tok = self.advance();
                        let name = match f_tok.kind {
                            TokenKind::String(s) => s,
                            TokenKind::Ident(s) => s,
                            _ => {
                                return Err(PvgError::parse_line(
                                    f_tok.line,
                                    "Expected \"nearest\" or \"linear\" for canvas filter.",
                                ));
                            }
                        };
                        pixel_filter = match name.to_lowercase().as_str() {
                            "nearest" => PixelFilter::Nearest,
                            _ => PixelFilter::Linear,
                        };
                    }
                    TokenKind::Newline => {
                        self.advance();
                    }
                    other => {
                        return Err(PvgError::parse(
                            self.peek().line,
                            self.peek().col,
                            format!("Invalid canvas property {:?}", other),
                        ));
                    }
                }
                self.skip_newlines();
            }
            self.match_kind(&TokenKind::Dedent);
        }
        self.skip_newlines();

        // 3. Document statements (+ top-level param / pattern declarations)
        let mut statements = Vec::new();
        let mut params = Vec::new();
        let mut patterns = Vec::new();
        while self.peek_kind() != &TokenKind::Eof {
            if self.peek_kind() == &TokenKind::Newline {
                self.advance();
                continue;
            }
            if self.peek_kind() == &TokenKind::Param {
                params.push(self.parse_param_decl()?);
                self.skip_newlines();
                continue;
            }
            if self.peek_kind() == &TokenKind::Pattern {
                patterns.push(self.parse_pattern_def()?);
                self.skip_newlines();
                continue;
            }
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }

        Ok(Document {
            version,
            canvas: CanvasDecl { width, height, background: bg, snap, pixel_filter },
            params,
            patterns,
            statements,
        })
    }

    fn parse_statement(&mut self) -> Result<Stmt, PvgError> {
        let peek_tok = self.peek().clone();
        match peek_tok.kind {
            TokenKind::Set => {
                self.advance();
                let tok = self.advance();
                let name = match soft_ident(&tok.kind) {
                    Some(n) => n,
                    None => {
                        return Err(PvgError::parse(
                            tok.line,
                            tok.col,
                            format!("Expected variable name after 'set', found {:?}", tok.kind),
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
                let tok = self.advance();
                let name = match soft_ident(&tok.kind) {
                    Some(n) => n,
                    None => return Err(PvgError::parse_line(self.peek().line, "Expected function name.")),
                };
                self.expect(TokenKind::LParen)?;
                let mut params = Vec::new();
                if self.peek_kind() != &TokenKind::RParen {
                    loop {
                        if let Some(p) = soft_ident(&self.advance().kind) {
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
                let tok = self.advance();
                let var = match soft_ident(&tok.kind) {
                    Some(n) => n,
                    None => return Err(PvgError::parse_line(self.peek().line, "Expected loop variable name.")),
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
            TokenKind::Clip => {
                self.advance();
                self.skip_newlines();
                self.parse_clip()
            }
            TokenKind::Param => {
                // `param` inside a block behaves like `set` with a default that
                // the host may override; evaluate identically at runtime.
                let decl = self.parse_param_decl()?;
                Ok(Stmt::Set(decl.name, decl.default))
            }
            TokenKind::Pattern => {
                return Err(PvgError::parse_line(
                    self.peek().line,
                    "Pattern blocks must be declared at top level.",
                ));
            }
            TokenKind::Sprite => {
                self.advance();
                self.skip_newlines();
                self.parse_sprite()
            }
            TokenKind::Spline => {
                self.advance();
                self.skip_newlines();
                self.parse_spline()
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
            // Soft keywords (`row`, `data`, `filter`, `snap`, `palette`) in
            // statement position: only valid as function calls (`row(...)`).
            // (Block openers `param`/`pattern`/`sprite`/`spline` are matched by
            // their own arms above.)
            TokenKind::Row | TokenKind::Data | TokenKind::Filter | TokenKind::Snap | TokenKind::Palette => {
                let name = soft_ident(self.peek_kind()).unwrap();
                let next_is_lparen =
                    matches!(self.tokens.get(self.pos + 1).map(|t| &t.kind), Some(TokenKind::LParen));
                if !next_is_lparen {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!(
                            "Unexpected statement token {:?} (reserved word; rename the variable if you meant a value).",
                            self.peek_kind()
                        ),
                    ));
                }
                self.advance();
                self.expect(TokenKind::LParen)?;
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
                Ok(Stmt::Call(name, args))
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

    /// Parses `fill`/`stroke` paint: solid expression, 0.2 gradient, or `pattern name`.
    fn parse_paint_expr(&mut self) -> Result<Expr, PvgError> {
        if self.peek_kind() == &TokenKind::Pattern {
            // Ambiguity: `pattern name` (tile ref) vs a variable literally
            // named `pattern`. A tile ref is always followed by a name, so
            // rewind and parse an expression when it isn't.
            let save = self.pos;
            self.advance();
            if let Some(name) = soft_ident(self.peek_kind()) {
                // Guard the degenerate `fill pattern pattern`... still a ref.
                self.advance();
                return Ok(Expr::Pattern(name));
            }
            self.pos = save;
        }
        match self.peek_kind() {
            TokenKind::Linear => {
                self.advance();
                let start = self.parse_expression()?;
                let end = self.parse_expression()?;
                let stops = self.try_parse_gradient_stops()?;
                Ok(Expr::Linear { start: Box::new(start), end: Box::new(end), stops })
            }
            TokenKind::Radial => {
                self.advance();
                let center = self.parse_expression()?;
                let radius = self.parse_expression()?;
                let focal = if self.peek_kind() == &TokenKind::LBracket {
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                let stops = self.try_parse_gradient_stops()?;
                Ok(Expr::Radial { center: Box::new(center), radius: Box::new(radius), focal, stops })
            }
            TokenKind::Angular => {
                self.advance();
                let center = self.parse_expression()?;
                let start_angle = self.parse_expression()?;
                let stops = self.try_parse_gradient_stops()?;
                Ok(Expr::Angular { center: Box::new(center), start_angle: Box::new(start_angle), stops })
            }
            _ => self.parse_expression(),
        }
    }

    /// Consumes an optional nested `stop` block after a gradient header.
    /// Returns empty vec when the next tokens are not a deeper `stop` block.
    fn try_parse_gradient_stops(&mut self) -> Result<Vec<GradientStop>, PvgError> {
        if self.peek_kind() != &TokenKind::Newline {
            return Ok(Vec::new());
        }
        // Lookahead: Newline(s) Indent Newline(s) Stop
        let mut j = self.pos + 1;
        while j < self.tokens.len() && self.tokens[j].kind == TokenKind::Newline {
            j += 1;
        }
        if j >= self.tokens.len() || self.tokens[j].kind != TokenKind::Indent {
            return Ok(Vec::new());
        }
        j += 1;
        while j < self.tokens.len() && self.tokens[j].kind == TokenKind::Newline {
            j += 1;
        }
        if j >= self.tokens.len() || self.tokens[j].kind != TokenKind::Stop {
            return Ok(Vec::new());
        }
        // Commit: consume newlines + indent
        while self.peek_kind() == &TokenKind::Newline {
            self.advance();
        }
        self.expect(TokenKind::Indent)?;
        let mut stops = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek_kind() == &TokenKind::Dedent {
                self.advance();
                break;
            }
            if self.peek_kind() == &TokenKind::Eof {
                break;
            }
            self.expect(TokenKind::Stop)?;
            let offset = self.parse_expression()?;
            let color = self.parse_expression()?;
            stops.push(GradientStop { offset, color });
            self.skip_newlines();
        }
        Ok(stops)
    }

    fn parse_dash_array(&mut self) -> Result<Vec<Expr>, PvgError> {
        self.expect(TokenKind::LBracket)?;
        let mut items = Vec::new();
        self.skip_newlines();
        if self.peek_kind() != &TokenKind::RBracket {
            loop {
                items.push(self.parse_expression()?);
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RBracket)?;
        Ok(items)
    }

    fn parse_shadow_expr(&mut self) -> Result<ShadowExpr, PvgError> {
        let offset = self.parse_expression()?;
        let radius = self.parse_expression()?;
        let color = self.parse_expression()?;
        Ok(ShadowExpr { offset, radius, color })
    }

    fn parse_glow_expr(&mut self) -> Result<GlowExpr, PvgError> {
        let radius = self.parse_expression()?;
        let color = self.parse_expression()?;
        Ok(GlowExpr { radius, color })
    }

    fn parse_circle(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut center = None;
        let mut radius = None;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Center => { self.advance(); center = Some(self.parse_expression()?); }
                TokenKind::Radius => { self.advance(); radius = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Cap => { self.advance(); cap = Some(self.parse_expression()?); }
                TokenKind::Join => { self.advance(); join = Some(self.parse_expression()?); }
                TokenKind::Miter => { self.advance(); miter = Some(self.parse_expression()?); }
                TokenKind::Dash => { self.advance(); dash = Some(self.parse_dash_array()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
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
        Ok(Stmt::Circle(CircleNode { center, radius, fill, stroke, width, opacity, cap, join, miter, dash, align, blur, shadow, glow, blend }))
    }

    fn parse_ellipse(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut center = None;
        let mut radius = None;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Center => { self.advance(); center = Some(self.parse_expression()?); }
                TokenKind::Radius => { self.advance(); radius = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Cap => { self.advance(); cap = Some(self.parse_expression()?); }
                TokenKind::Join => { self.advance(); join = Some(self.parse_expression()?); }
                TokenKind::Miter => { self.advance(); miter = Some(self.parse_expression()?); }
                TokenKind::Dash => { self.advance(); dash = Some(self.parse_dash_array()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
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
        Ok(Stmt::Ellipse(EllipseNode { center, radius, fill, stroke, width, opacity, cap, join, miter, dash, align, blur, shadow, glow, blend }))
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
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => { self.advance(); pos = Some(self.parse_expression()?); }
                TokenKind::Size => { self.advance(); size = Some(self.parse_expression()?); }
                TokenKind::Radius => { self.advance(); radius = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Cap => { self.advance(); cap = Some(self.parse_expression()?); }
                TokenKind::Join => { self.advance(); join = Some(self.parse_expression()?); }
                TokenKind::Miter => { self.advance(); miter = Some(self.parse_expression()?); }
                TokenKind::Dash => { self.advance(); dash = Some(self.parse_dash_array()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
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
        Ok(Stmt::Rectangle(RectNode { pos, size, radius, fill, stroke, width, opacity, cap, join, miter, dash, align, blur, shadow, glow, blend }))
    }

    fn parse_line(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut from = None;
        let mut to = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::From => { self.advance(); from = Some(self.parse_expression()?); }
                TokenKind::To => { self.advance(); to = Some(self.parse_expression()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Cap => { self.advance(); cap = Some(self.parse_expression()?); }
                TokenKind::Join => { self.advance(); join = Some(self.parse_expression()?); }
                TokenKind::Miter => { self.advance(); miter = Some(self.parse_expression()?); }
                TokenKind::Dash => { self.advance(); dash = Some(self.parse_dash_array()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
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
        Ok(Stmt::Line(LineNode { from, to, stroke, width, opacity, cap, join, miter, dash, align, blur, shadow, glow, blend }))
    }

    fn parse_polygon(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut points = Vec::new();
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Points => {
                    self.advance();
                    while self.peek_kind() == &TokenKind::LBracket {
                        points.push(self.parse_expression()?);
                    }
                }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Cap => { self.advance(); cap = Some(self.parse_expression()?); }
                TokenKind::Join => { self.advance(); join = Some(self.parse_expression()?); }
                TokenKind::Miter => { self.advance(); miter = Some(self.parse_expression()?); }
                TokenKind::Dash => { self.advance(); dash = Some(self.parse_dash_array()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
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

        Ok(Stmt::Polygon(PolygonNode { points, fill, stroke, width, opacity, cap, join, miter, dash, align, blur, shadow, glow, blend }))
    }

    /// Parses one item inside a `path` body: a style prop (applied to the
    /// out params), a draw command, or a control-flow node. Returns `Ok(true)`
    /// when a draw/control command was pushed.
    #[allow(clippy::too_many_arguments)]
    fn parse_path_item(
        &mut self,
        fill: &mut Option<Expr>,
        stroke: &mut Option<Expr>,
        width: &mut Option<Expr>,
        opacity: &mut Option<Expr>,
        cap: &mut Option<Expr>,
        join: &mut Option<Expr>,
        miter: &mut Option<Expr>,
        dash: &mut Option<Vec<Expr>>,
        align: &mut Option<Expr>,
        blur: &mut Option<Expr>,
        shadow: &mut Option<ShadowExpr>,
        glow: &mut Option<GlowExpr>,
        blend: &mut Option<Expr>,
        commands: &mut Vec<PathCommand>,
    ) -> Result<(), PvgError> {
        match self.peek_kind() {
            TokenKind::Set => {
                self.advance();
                let tok = self.advance();
                let name = match soft_ident(&tok.kind) {
                    Some(n) => n,
                    None => {
                        return Err(PvgError::parse(
                            tok.line,
                            tok.col,
                            format!("Expected variable name after 'set', found {:?}", tok.kind),
                        ));
                    }
                };
                self.expect(TokenKind::Equal)?;
                let expr = self.parse_expression()?;
                commands.push(PathCommand::Set(name, expr));
            }
            TokenKind::For => {
                self.advance();
                let tok = self.advance();
                let var = match soft_ident(&tok.kind) {
                    Some(n) => n,
                    None => {
                        return Err(PvgError::parse_line(
                            self.peek().line,
                            "Expected loop variable name.",
                        ));
                    }
                };
                self.expect(TokenKind::From)?;
                let from = self.parse_expression()?;
                self.expect(TokenKind::To)?;
                let to = self.parse_expression()?;
                let mut step = None;
                if self.match_kind(&TokenKind::Step) {
                    step = Some(self.parse_expression()?);
                }
                self.skip_newlines();
                let body = self.parse_path_block()?;
                commands.push(PathCommand::For { var, from, to, step, body });
            }
            TokenKind::While => {
                self.advance();
                let cond = self.parse_expression()?;
                self.skip_newlines();
                let body = self.parse_path_block()?;
                commands.push(PathCommand::While { cond, body });
            }
            TokenKind::If => {
                commands.push(self.parse_path_if()?);
            }
                TokenKind::Fill => { self.advance(); *fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); *stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); *width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); *opacity = Some(self.parse_expression()?); }
                TokenKind::Cap => { self.advance(); *cap = Some(self.parse_expression()?); }
                TokenKind::Join => { self.advance(); *join = Some(self.parse_expression()?); }
                TokenKind::Miter => { self.advance(); *miter = Some(self.parse_expression()?); }
                TokenKind::Dash => { self.advance(); *dash = Some(self.parse_dash_array()?); }
                TokenKind::Align => { self.advance(); *align = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); *blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); *shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); *glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); *blend = Some(self.parse_expression()?); }
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
                    // An expression where a path command was expected almost
                    // always means someone wrote `cond ? start a : line b`.
                    // Ternary branches must be expressions — point at if/else.
                    let looks_like_expr = matches!(
                        other,
                        TokenKind::Ident(_)
                            | TokenKind::Number(_)
                            | TokenKind::String(_)
                            | TokenKind::Color(_)
                            | TokenKind::LBracket
                            | TokenKind::LParen
                            | TokenKind::Minus
                            | TokenKind::Not
                            | TokenKind::Row
                            | TokenKind::Data
                            | TokenKind::Filter
                            | TokenKind::Snap
                            | TokenKind::Palette
                            | TokenKind::Param
                            | TokenKind::Pattern
                            | TokenKind::Sprite
                            | TokenKind::Spline
                    );
                    let msg = if looks_like_expr {
                        format!(
                            "Invalid path property/command {:?}. Hint: path bodies take draw commands (start/line/quad/curve/arc/close), set, or if/for/while — `? :` branches must be expressions, use `if <cond>` / `else` for conditional points.",
                            other
                        )
                    } else {
                        format!("Invalid path property/command {:?}", other)
                    };
                    return Err(PvgError::parse(self.peek().line, self.peek().col, msg));
                }
            }
            Ok(())
        }

    /// Parses an indented block of path items (draw commands, `set`, and
    /// control flow) used by `path` bodies and nested path control nodes.
    fn parse_path_block(&mut self) -> Result<Vec<PathCommand>, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;
        let mut commands = Vec::new();
        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            self.parse_path_item(
                &mut fill, &mut stroke, &mut width, &mut opacity, &mut cap, &mut join,
                &mut miter, &mut dash, &mut align, &mut blur, &mut shadow, &mut glow,
                &mut blend, &mut commands,
            )?;
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;
        // Style props inside nested control blocks apply to the whole path
        // (they share the path's style); re-emit them as leading no-op
        // commands is unnecessary — instead, validation: nested style props
        // are simply ignored here because the outer `parse_path` already
        // collected its own. To keep semantics simple, nested blocks may not
        // set style props: surface them as errors.
        if fill.is_some()
            || stroke.is_some()
            || width.is_some()
            || opacity.is_some()
            || cap.is_some()
            || join.is_some()
            || miter.is_some()
            || dash.is_some()
            || align.is_some()
            || blur.is_some()
            || shadow.is_some()
            || glow.is_some()
            || blend.is_some()
        {
            return Err(PvgError::parse_line(
                self.peek().line,
                "Style props (fill, stroke, ...) must be set directly in the path body, not inside for/while/if.",
            ));
        }
        Ok(commands)
    }

    /// Parses `if <cond>` + block + optional `else` (or `else if` chain)
    /// inside a `path` body.
    fn parse_path_if(&mut self) -> Result<PathCommand, PvgError> {
        self.expect(TokenKind::If)?;
        let cond = self.parse_expression()?;
        self.skip_newlines();
        let then_body = self.parse_path_block()?;
        let mut else_body = Vec::new();
        self.skip_newlines();
        if self.match_kind(&TokenKind::Else) {
            if self.peek_kind() == &TokenKind::If {
                else_body.push(self.parse_path_if()?);
            } else {
                self.skip_newlines();
                else_body = self.parse_path_block()?;
            }
        }
        Ok(PathCommand::If { cond, then_body, else_body })
    }

    fn parse_path(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut fill = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut align = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;
        let mut commands = Vec::new();

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            self.parse_path_item(
                &mut fill, &mut stroke, &mut width, &mut opacity, &mut cap, &mut join,
                &mut miter, &mut dash, &mut align, &mut blur, &mut shadow, &mut glow,
                &mut blend, &mut commands,
            )?;
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        Ok(Stmt::Path(PathNode { fill, stroke, width, opacity, cap, join, miter, dash, align, blur, shadow, glow, blend, commands }))
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
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => { self.advance(); pos = Some(self.parse_expression()?); }
                TokenKind::Content | TokenKind::Text => { self.advance(); content = Some(self.parse_expression()?); }
                TokenKind::Size => { self.advance(); size = Some(self.parse_expression()?); }
                TokenKind::Font => { self.advance(); font = Some(self.parse_expression()?); }
                TokenKind::Align => { self.advance(); align = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Width => { self.advance(); width = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
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
        Ok(Stmt::Text(TextNode { pos, content, size, font, align, fill, stroke, width, opacity, blur, shadow, glow, blend }))
    }

    fn parse_group(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut pos = None;
        let mut rot = None;
        let mut scale = None;
        let mut opacity = None;
        let mut fill = None;
        let mut stroke = None;
        let mut blend = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut body = Vec::new();

        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => { self.advance(); pos = Some(self.parse_expression()?); }
                TokenKind::Rot => { self.advance(); rot = Some(self.parse_expression()?); }
                TokenKind::Scale => { self.advance(); scale = Some(self.parse_expression()?); }
                TokenKind::Opacity => { self.advance(); opacity = Some(self.parse_expression()?); }
                TokenKind::Fill => { self.advance(); fill = Some(self.parse_paint_expr()?); }
                TokenKind::Stroke => { self.advance(); stroke = Some(self.parse_paint_expr()?); }
                TokenKind::Blend => { self.advance(); blend = Some(self.parse_expression()?); }
                TokenKind::Blur => { self.advance(); blur = Some(self.parse_expression()?); }
                TokenKind::Shadow => { self.advance(); shadow = Some(self.parse_shadow_expr()?); }
                TokenKind::Glow => { self.advance(); glow = Some(self.parse_glow_expr()?); }
                TokenKind::Newline => { self.advance(); }
                _ => { body.push(self.parse_statement()?); }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;

        Ok(Stmt::Group(GroupNode { pos, rot, scale, opacity, fill, stroke, blend, blur, shadow, glow, body }))
    }

    fn parse_clip(&mut self) -> Result<Stmt, PvgError> {
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

        if stmts.is_empty() {
            return Err(PvgError::parse_line(self.peek().line, "Clip block requires a mask shape plus content."));
        }
        let mask = Box::new(stmts.remove(0));
        match *mask {
            Stmt::Circle(_) | Stmt::Ellipse(_) | Stmt::Rectangle(_)
            | Stmt::Polygon(_) | Stmt::Path(_) | Stmt::Line(_) | Stmt::Text(_) => {}
            _ => {
                return Err(PvgError::parse_line(
                    self.peek().line,
                    "Clip mask must be a geometric shape (circle, ellipse, rectangle, polygon, path, line, text).",
                ));
            }
        }
        Ok(Stmt::Clip { mask, content: stmts })
    }

    /// `param name: default` or `param name = default` (host-overridable uniform).
    fn parse_param_decl(&mut self) -> Result<ParamDecl, PvgError> {
        self.expect(TokenKind::Param)?;
        let tok = self.advance();
        let name = match soft_ident(&tok.kind) {
            Some(n) => n,
            None => {
                return Err(PvgError::parse(
                    tok.line,
                    tok.col,
                    format!("Expected param name, found {:?}", tok.kind),
                ));
            }
        };
        if !(self.match_kind(&TokenKind::Colon) || self.match_kind(&TokenKind::Equal)) {
            return Err(PvgError::parse_line(
                self.peek().line,
                "Expected ':' or '=' after param name (e.g. `param health: 0.75`).",
            ));
        }
        let default = self.parse_expression()?;
        Ok(ParamDecl { name, default })
    }

    /// `pattern name w h` + indented body block (repeatable tile).
    fn parse_pattern_def(&mut self) -> Result<PatternDef, PvgError> {
        self.expect(TokenKind::Pattern)?;
        let tok = self.advance();
        let name = match soft_ident(&tok.kind) {
            Some(n) => n,
            None => {
                return Err(PvgError::parse(
                    tok.line,
                    tok.col,
                    format!("Expected pattern name, found {:?}", tok.kind),
                ));
            }
        };
        let w = match self.advance().kind {
            TokenKind::Number(v) => v,
            _ => return Err(PvgError::parse_line(self.peek().line, "Expected pattern tile width.")),
        };
        let h = match self.advance().kind {
            TokenKind::Number(v) => v,
            _ => return Err(PvgError::parse_line(self.peek().line, "Expected pattern tile height.")),
        };
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(PatternDef { name, width: w, height: h, body })
    }

    fn parse_palette_array(&mut self) -> Result<Vec<Expr>, PvgError> {
        self.expect(TokenKind::LBracket)?;
        let mut items = Vec::new();
        self.skip_newlines();
        if self.peek_kind() != &TokenKind::RBracket {
            loop {
                items.push(self.parse_expression()?);
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RBracket)?;
        Ok(items)
    }

    fn parse_sprite(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut pos = None;
        let mut palette = Vec::new();
        let mut rows: Vec<String> = Vec::new();
        let mut scale = None;
        let mut opacity = None;
        let mut blend = None;
        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Pos => {
                    self.advance();
                    pos = Some(self.parse_expression()?);
                }
                TokenKind::Palette => {
                    self.advance();
                    palette = self.parse_palette_array()?;
                }
                TokenKind::Data | TokenKind::Row => {
                    self.advance();
                    let tok = self.advance();
                    match tok.kind {
                        TokenKind::String(s) => {
                            // Accept one row per literal, or a whole
                            // triple-quoted block: split on newlines and keep
                            // non-blank rows (blank = pure whitespace).
                            let mut kept = 0;
                            for row in s.split('\n') {
                                let row = row.strip_suffix('\r').unwrap_or(row);
                                if row.trim().is_empty() {
                                    continue;
                                }
                                rows.push(row.to_string());
                                kept += 1;
                            }
                            if kept == 0 {
                                return Err(PvgError::parse_line(
                                    tok.line,
                                    "Sprite `data` block has no pixel rows.",
                                ));
                            }
                        }
                        _ => {
                            return Err(PvgError::parse_line(
                                tok.line,
                                "Expected quoted pixel row after `data` (e.g. data \"..11..\" or a \"\"\" block).",
                            ));
                        }
                    }
                }
                TokenKind::Scale => {
                    self.advance();
                    scale = Some(self.parse_expression()?);
                }
                TokenKind::Opacity => {
                    self.advance();
                    opacity = Some(self.parse_expression()?);
                }
                TokenKind::Blend => {
                    self.advance();
                    blend = Some(self.parse_expression()?);
                }
                TokenKind::Newline => {
                    self.advance();
                }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid sprite property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;
        let pos = pos.ok_or_else(|| PvgError::parse_line(self.peek().line, "Sprite requires 'pos [x, y]'"))?;
        if palette.is_empty() {
            return Err(PvgError::parse_line(self.peek().line, "Sprite requires 'palette [c0, c1, ...]'."));
        }
        if rows.is_empty() {
            return Err(PvgError::parse_line(self.peek().line, "Sprite requires at least one `data \"...\"` row."));
        }
        Ok(Stmt::Sprite(SpriteNode { pos, palette, rows, scale, opacity, blend }))
    }

    fn parse_spline(&mut self) -> Result<Stmt, PvgError> {
        self.expect(TokenKind::Indent)?;
        let mut points = None;
        let mut pos = None;
        let mut size = None;
        let mut stroke = None;
        let mut width = None;
        let mut opacity = None;
        let mut cap = None;
        let mut join = None;
        let mut miter = None;
        let mut dash: Option<Vec<Expr>> = None;
        let mut blur = None;
        let mut shadow = None;
        let mut glow = None;
        let mut blend = None;
        while self.peek_kind() != &TokenKind::Dedent && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::Points => {
                    self.advance();
                    points = Some(self.parse_expression()?);
                }
                TokenKind::Pos => {
                    self.advance();
                    pos = Some(self.parse_expression()?);
                }
                TokenKind::Size => {
                    self.advance();
                    size = Some(self.parse_expression()?);
                }
                TokenKind::Stroke => {
                    self.advance();
                    stroke = Some(self.parse_paint_expr()?);
                }
                TokenKind::Width => {
                    self.advance();
                    width = Some(self.parse_expression()?);
                }
                TokenKind::Opacity => {
                    self.advance();
                    opacity = Some(self.parse_expression()?);
                }
                TokenKind::Cap => {
                    self.advance();
                    cap = Some(self.parse_expression()?);
                }
                TokenKind::Join => {
                    self.advance();
                    join = Some(self.parse_expression()?);
                }
                TokenKind::Miter => {
                    self.advance();
                    miter = Some(self.parse_expression()?);
                }
                TokenKind::Dash => {
                    self.advance();
                    dash = Some(self.parse_dash_array()?);
                }
                TokenKind::Blur => {
                    self.advance();
                    blur = Some(self.parse_expression()?);
                }
                TokenKind::Shadow => {
                    self.advance();
                    shadow = Some(self.parse_shadow_expr()?);
                }
                TokenKind::Glow => {
                    self.advance();
                    glow = Some(self.parse_glow_expr()?);
                }
                TokenKind::Blend => {
                    self.advance();
                    blend = Some(self.parse_expression()?);
                }
                TokenKind::Newline => {
                    self.advance();
                }
                other => {
                    return Err(PvgError::parse(
                        self.peek().line,
                        self.peek().col,
                        format!("Invalid spline property {:?}", other),
                    ));
                }
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::Dedent)?;
        let points =
            points.ok_or_else(|| PvgError::parse_line(self.peek().line, "Spline requires 'points <array>'"))?;
        Ok(Stmt::Spline(SplineNode {
            points, pos, size, stroke, width, opacity, cap, join, miter, dash, blur, shadow, glow, blend,
        }))
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
                // Bracket list: exactly 2 elements = Vec2 (back-compat);
                // any other arity = Array literal. Use `array(a, b)` for
                // explicit 2-element arrays.
                let first = self.parse_expression()?;
                if self.peek_kind() != &TokenKind::Comma {
                    self.expect(TokenKind::RBracket)?;
                    return Ok(Expr::Array(vec![first]));
                }
                let mut items = vec![first];
                while self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                    // Allow trailing comma: `[a, b, ]`.
                    if self.peek_kind() == &TokenKind::RBracket {
                        break;
                    }
                    items.push(self.parse_expression()?);
                }
                self.expect(TokenKind::RBracket)?;
                // Two scalar components = Vec2 (back-compat for positions).
                // Nested compounds (`[[10, 10], [90, 90]]`) are arrays, since
                // a Vec2 of Vec2s is meaningless. Use `array(a, b)` for an
                // explicit 2-element data array of scalar expressions.
                let is_compound = |e: &Expr| matches!(e, Expr::Vec2(..) | Expr::Array(..));
                if items.len() == 2 && !items.iter().any(is_compound) {
                    let mut it = items.into_iter();
                    let x = it.next().unwrap();
                    let y = it.next().unwrap();
                    Ok(Expr::Vec2(Box::new(x), Box::new(y)))
                } else {
                    Ok(Expr::Array(items))
                }
            }
            TokenKind::LParen => {
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            kind => {
                // Plain identifiers and soft keywords (`row`, `data`, ...)
                // alike: variables, `true`/`false`, and calls.
                let s = match soft_ident(&kind) {
                    Some(n) => n,
                    None => {
                        return Err(PvgError::parse(
                            tok.line,
                            tok.col,
                            format!("Unexpected token in expression: {:?}", kind),
                        ));
                    }
                };
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
        }
    }
}