use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    LParen,
    RParen,
    Quote,
    String(String),
    Atom(String),
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            ';' => {
                // Comment - skip to end of line
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '\'' => {
                tokens.push(Token::Quote);
                chars.next();
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('\\') => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('\\') => s.push('\\'),
                            Some('"') => s.push('"'),
                            Some(c) => s.push(c),
                            None => return Err("unexpected end of string".to_string()),
                        },
                        Some('"') => break,
                        Some(c) => s.push(c),
                        None => return Err("unterminated string".to_string()),
                    }
                }
                tokens.push(Token::String(s));
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
                }
                tokens.push(Token::Atom(atom));
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

    match &tokens[pos] {
        Token::LParen => parse_list(tokens, pos + 1),
        Token::RParen => Err("unexpected ')'".to_string()),
        Token::Quote => {
            let (expr, new_pos) = parse_expr(tokens, pos + 1)?;
            Ok((
                Value::List(vec![Value::Symbol("quote".to_string()), expr]),
                new_pos,
            ))
        }
        Token::String(s) => Ok((Value::String(s.clone()), pos + 1)),
        Token::Atom(s) => Ok((parse_atom(s), pos + 1)),
    }
}

fn parse_list(tokens: &[Token], mut pos: usize) -> Result<(Value, usize), String> {
    let mut items = Vec::new();

    loop {
        if pos >= tokens.len() {
            return Err("unterminated list".to_string());
        }
        if tokens[pos] == Token::RParen {
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
