use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    fn new(line: usize, col: usize) -> Self {
        Span { line, col }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LParen,
    RParen,
    Quote,
    String(String),
    Atom(String),
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut line = 1;
    let mut col = 1;

    while let Some(&ch) = chars.peek() {
        let start_line = line;
        let start_col = col;

        match ch {
            '\n' => {
                chars.next();
                line += 1;
                col = 1;
            }
            ' ' | '\t' | '\r' => {
                chars.next();
                col += 1;
            }
            ';' => {
                // Comment - skip to end of line
                while let Some(&c) = chars.peek() {
                    chars.next();
                    col += 1;
                    if c == '\n' {
                        line += 1;
                        col = 1;
                        break;
                    }
                }
            }
            '(' => {
                tokens.push(Token {
                    kind: TokenKind::LParen,
                    span: Span::new(start_line, start_col),
                });
                chars.next();
                col += 1;
            }
            ')' => {
                tokens.push(Token {
                    kind: TokenKind::RParen,
                    span: Span::new(start_line, start_col),
                });
                chars.next();
                col += 1;
            }
            '\'' => {
                tokens.push(Token {
                    kind: TokenKind::Quote,
                    span: Span::new(start_line, start_col),
                });
                chars.next();
                col += 1;
            }
            '"' => {
                chars.next();
                col += 1;
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('\n') => {
                            s.push('\n');
                            line += 1;
                            col = 1;
                        }
                        Some('\\') => {
                            col += 1;
                            match chars.next() {
                                Some('n') => {
                                    s.push('\n');
                                    col += 1;
                                }
                                Some('t') => {
                                    s.push('\t');
                                    col += 1;
                                }
                                Some('\\') => {
                                    s.push('\\');
                                    col += 1;
                                }
                                Some('"') => {
                                    s.push('"');
                                    col += 1;
                                }
                                Some(c) => {
                                    s.push(c);
                                    col += 1;
                                }
                                None => {
                                    return Err(format!(
                                        "unexpected end of string at line {}, col {}",
                                        start_line, start_col
                                    ))
                                }
                            }
                        }
                        Some('"') => {
                            col += 1;
                            break;
                        }
                        Some(c) => {
                            s.push(c);
                            col += 1;
                        }
                        None => {
                            return Err(format!(
                                "unterminated string at line {}, col {}",
                                start_line, start_col
                            ))
                        }
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::String(s),
                    span: Span::new(start_line, start_col),
                });
            }
            _ => {
                let mut atom = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' '
                        || c == '\t'
                        || c == '\n'
                        || c == '\r'
                        || c == '('
                        || c == ')'
                        || c == ';'
                    {
                        break;
                    }
                    atom.push(c);
                    chars.next();
                    col += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Atom(atom),
                    span: Span::new(start_line, start_col),
                });
            }
        }
    }

    Ok(tokens)
}

pub fn parse(input: &str) -> Result<Vec<Value>, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let mut exprs = Vec::new();

    while pos < tokens.len() {
        let (expr, new_pos) = parse_expr(&tokens, pos)?;
        exprs.push(expr);
        pos = new_pos;
    }

    Ok(exprs)
}

fn parse_expr(tokens: &[Token], pos: usize) -> Result<(Value, usize), String> {
    if pos >= tokens.len() {
        return Err("unexpected end of input".to_string());
    }

    let token = &tokens[pos];
    let span = token.span;

    match &token.kind {
        TokenKind::LParen => parse_list(tokens, pos + 1, span),
        TokenKind::RParen => Err(format!(
            "unexpected ')' at line {}, col {}",
            span.line, span.col
        )),
        TokenKind::Quote => {
            let (expr, new_pos) = parse_expr(tokens, pos + 1)?;
            Ok((
                Value::List(vec![Value::Symbol("quote".to_string()), expr]),
                new_pos,
            ))
        }
        TokenKind::String(s) => Ok((Value::String(s.clone()), pos + 1)),
        TokenKind::Atom(s) => Ok((parse_atom(s), pos + 1)),
    }
}

fn parse_list(tokens: &[Token], mut pos: usize, open_span: Span) -> Result<(Value, usize), String> {
    let mut items = Vec::new();

    loop {
        if pos >= tokens.len() {
            return Err(format!(
                "unterminated list starting at line {}, col {}",
                open_span.line, open_span.col
            ));
        }
        if tokens[pos].kind == TokenKind::RParen {
            return Ok((Value::List(items), pos + 1));
        }
        let (expr, new_pos) = parse_expr(tokens, pos)?;
        items.push(expr);
        pos = new_pos;
    }
}

fn parse_atom(s: &str) -> Value {
    // The order here is important since this is returning early.
    // If this isn't an i64 it will try as a f64 and then try for booleans.

    if let Ok(n) = s.parse::<i64>() {
        return Value::Int(n);
    }

    if let Ok(n) = s.parse::<f64>() {
        return Value::Float(n);
    }

    match s {
        "true" | "#t" => Value::Bool(true),
        "false" | "#f" => Value::Bool(false),
        "nil" => Value::Nil,
        _ => Value::Symbol(s.to_string()),
    }
}
