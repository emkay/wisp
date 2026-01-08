use crate::value::Value;

/// Used to denote what line and column a token shows up in the source code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Span { line, col }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}, col {}", self.line, self.col)
    }
}

/// An expression with optional source location.
/// Used during parsing and evaluation to provide better error messages.
#[derive(Debug, Clone)]
pub struct Expr {
    pub value: Value,
    pub span: Option<Span>,
}

impl Expr {
    pub fn new(value: Value, span: Span) -> Self {
        Expr { value, span: Some(span) }
    }

    pub fn runtime(value: Value) -> Self {
        Expr { value, span: None }
    }

    /// Format an error message, including span if available
    pub fn error(&self, msg: &str) -> String {
        match self.span {
            Some(span) => format!("{} at {}", msg, span),
            None => msg.to_string(),
        }
    }
}

/// A [`Token`] keeps track of what kind of token it is by using [`TokenKind`] and where it is in
/// the source code by giving it a [`Span`]. Knowing where the token is can be super useful for
/// debugging.
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

/// Takes Wisp code and turns it into a [`Vec`] of [`Token`]'s that are a specific [`TokenKind`].
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

/// This is the entry point. Takes a string of Wisp code and returns an AST in the form of a [`Vec`] of [`Expr`]'s.
pub fn parse(input: &str) -> Result<Vec<Expr>, String> {
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

fn parse_expr(tokens: &[Token], pos: usize) -> Result<(Expr, usize), String> {
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
            let (inner, new_pos) = parse_expr(tokens, pos + 1)?;
            let list = Value::List(vec![
                Value::Symbol("quote".to_string()),
                inner.value,
            ]);
            Ok((Expr::new(list, span), new_pos))
        }
        TokenKind::String(s) => Ok((Expr::new(Value::String(s.clone()), span), pos + 1)),
        TokenKind::Atom(s) => Ok((Expr::new(parse_atom(s), span), pos + 1)),
    }
}

fn parse_list(tokens: &[Token], mut pos: usize, open_span: Span) -> Result<(Expr, usize), String> {
    let mut items = Vec::new();

    loop {
        if pos >= tokens.len() {
            return Err(format!(
                "unterminated list starting at line {}, col {}",
                open_span.line, open_span.col
            ));
        }
        if tokens[pos].kind == TokenKind::RParen {
            return Ok((Expr::new(Value::List(items), open_span), pos + 1));
        }
        let (expr, new_pos) = parse_expr(tokens, pos)?;
        items.push(expr.value);
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

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Tokenizer tests =====

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_whitespace_only() {
        let tokens = tokenize("   \t\n  \r\n  ").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_parens() {
        let tokens = tokenize("()").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::LParen);
        assert_eq!(tokens[1].kind, TokenKind::RParen);
    }

    #[test]
    fn test_tokenize_nested_parens() {
        let tokens = tokenize("((()))").unwrap();
        assert_eq!(tokens.len(), 6);
    }

    #[test]
    fn test_tokenize_quote() {
        let tokens = tokenize("'x").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Quote);
        assert_eq!(tokens[1].kind, TokenKind::Atom("x".to_string()));
    }

    #[test]
    fn test_tokenize_simple_string() {
        let tokens = tokenize("\"hello\"").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String("hello".to_string()));
    }

    #[test]
    fn test_tokenize_string_with_escapes() {
        let tokens = tokenize(r#""hello\nworld\t!""#).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String("hello\nworld\t!".to_string()));
    }

    #[test]
    fn test_tokenize_string_with_escaped_quote() {
        let tokens = tokenize(r#""say \"hi\"""#).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String("say \"hi\"".to_string()));
    }

    #[test]
    fn test_tokenize_string_with_escaped_backslash() {
        let tokens = tokenize(r#""path\\to\\file""#).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String("path\\to\\file".to_string()));
    }

    #[test]
    fn test_tokenize_unterminated_string() {
        let result = tokenize("\"hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unterminated string"));
    }

    #[test]
    fn test_tokenize_unterminated_escape() {
        let result = tokenize("\"hello\\");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unexpected end of string"));
    }

    #[test]
    fn test_tokenize_atoms() {
        let tokens = tokenize("foo bar-baz").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Atom("foo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Atom("bar-baz".to_string()));
    }

    #[test]
    fn test_tokenize_numbers() {
        let tokens = tokenize("42 -17 3.14").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::Atom("42".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Atom("-17".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Atom("3.14".to_string()));
    }

    #[test]
    fn test_tokenize_comments() {
        let tokens = tokenize("; this is a comment\nfoo").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Atom("foo".to_string()));
    }

    #[test]
    fn test_tokenize_inline_comment() {
        let tokens = tokenize("(foo ; comment\nbar)").unwrap();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].kind, TokenKind::LParen);
        assert_eq!(tokens[1].kind, TokenKind::Atom("foo".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Atom("bar".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::RParen);
    }

    #[test]
    fn test_tokenize_multiline_string() {
        let tokens = tokenize("\"line1\nline2\"").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String("line1\nline2".to_string()));
    }

    #[test]
    fn test_tokenize_span_tracking() {
        let tokens = tokenize("(\n  x\n)").unwrap();
        assert_eq!(tokens[0].span, Span::new(1, 1));  // (
        assert_eq!(tokens[1].span, Span::new(2, 3));  // x
        assert_eq!(tokens[2].span, Span::new(3, 1));  // )
    }

    // ===== Parser tests =====

    #[test]
    fn test_parse_empty() {
        let exprs = parse("").unwrap();
        assert!(exprs.is_empty());
    }

    #[test]
    fn test_parse_integer() {
        let exprs = parse("42").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Int(42));
    }

    #[test]
    fn test_parse_negative_integer() {
        let exprs = parse("-17").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Int(-17));
    }

    #[test]
    fn test_parse_float() {
        let exprs = parse("3.14").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Float(3.14));
    }

    #[test]
    fn test_parse_negative_float() {
        let exprs = parse("-2.5").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Float(-2.5));
    }

    #[test]
    fn test_parse_booleans() {
        let exprs = parse("true false #t #f").unwrap();
        assert_eq!(exprs.len(), 4);
        assert_eq!(exprs[0].value, Value::Bool(true));
        assert_eq!(exprs[1].value, Value::Bool(false));
        assert_eq!(exprs[2].value, Value::Bool(true));
        assert_eq!(exprs[3].value, Value::Bool(false));
    }

    #[test]
    fn test_parse_nil() {
        let exprs = parse("nil").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Nil);
    }

    #[test]
    fn test_parse_symbol() {
        let exprs = parse("foo-bar").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Symbol("foo-bar".to_string()));
    }

    #[test]
    fn test_parse_symbol_with_special_chars() {
        let exprs = parse("null? string->symbol +").unwrap();
        assert_eq!(exprs.len(), 3);
        assert_eq!(exprs[0].value, Value::Symbol("null?".to_string()));
        assert_eq!(exprs[1].value, Value::Symbol("string->symbol".to_string()));
        assert_eq!(exprs[2].value, Value::Symbol("+".to_string()));
    }

    #[test]
    fn test_parse_string() {
        let exprs = parse("\"hello world\"").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_parse_empty_string() {
        let exprs = parse("\"\"").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::String("".to_string()));
    }

    #[test]
    fn test_parse_empty_list() {
        let exprs = parse("()").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::List(vec![]));
    }

    #[test]
    fn test_parse_simple_list() {
        let exprs = parse("(+ 1 2)").unwrap();
        assert_eq!(exprs.len(), 1);
        match &exprs[0].value {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::Symbol("+".to_string()));
                assert_eq!(items[1], Value::Int(1));
                assert_eq!(items[2], Value::Int(2));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_parse_nested_list() {
        let exprs = parse("(+ (* 2 3) 4)").unwrap();
        assert_eq!(exprs.len(), 1);
        match &exprs[0].value {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                match &items[1] {
                    Value::List(inner) => {
                        assert_eq!(inner.len(), 3);
                        assert_eq!(inner[0], Value::Symbol("*".to_string()));
                    }
                    _ => panic!("expected nested list"),
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_parse_quote_syntax() {
        let exprs = parse("'x").unwrap();
        assert_eq!(exprs.len(), 1);
        match &exprs[0].value {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Symbol("quote".to_string()));
                assert_eq!(items[1], Value::Symbol("x".to_string()));
            }
            _ => panic!("expected quoted list"),
        }
    }

    #[test]
    fn test_parse_quoted_list() {
        let exprs = parse("'(1 2 3)").unwrap();
        assert_eq!(exprs.len(), 1);
        match &exprs[0].value {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Symbol("quote".to_string()));
                match &items[1] {
                    Value::List(inner) => {
                        assert_eq!(inner.len(), 3);
                    }
                    _ => panic!("expected inner list"),
                }
            }
            _ => panic!("expected quoted list"),
        }
    }

    #[test]
    fn test_parse_multiple_exprs() {
        let exprs = parse("1 2 3").unwrap();
        assert_eq!(exprs.len(), 3);
        assert_eq!(exprs[0].value, Value::Int(1));
        assert_eq!(exprs[1].value, Value::Int(2));
        assert_eq!(exprs[2].value, Value::Int(3));
    }

    #[test]
    fn test_parse_unterminated_list() {
        let result = parse("(foo bar");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unterminated list"));
    }

    #[test]
    fn test_parse_unexpected_rparen() {
        let result = parse(")");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unexpected ')'"));
    }

    #[test]
    fn test_parse_mismatched_parens() {
        let result = parse("(foo))");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_complex_program() {
        let code = r#"
            (define (factorial n)
              (if (<= n 1)
                  1
                  (* n (factorial (- n 1)))))
        "#;
        let exprs = parse(code).unwrap();
        assert_eq!(exprs.len(), 1);
        match &exprs[0].value {
            Value::List(items) => {
                assert_eq!(items[0], Value::Symbol("define".to_string()));
            }
            _ => panic!("expected define list"),
        }
    }

    #[test]
    fn test_parse_preserves_span() {
        let exprs = parse("foo").unwrap();
        assert!(exprs[0].span.is_some());
        let span = exprs[0].span.unwrap();
        assert_eq!(span.line, 1);
        assert_eq!(span.col, 1);
    }

    #[test]
    fn test_parse_unicode_string() {
        let exprs = parse("\"héllo wörld 🌍\"").unwrap();
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::String("héllo wörld 🌍".to_string()));
    }

    #[test]
    fn test_parse_very_large_integer() {
        let exprs = parse("9223372036854775807").unwrap();  // i64::MAX
        assert_eq!(exprs.len(), 1);
        assert_eq!(exprs[0].value, Value::Int(9223372036854775807));
    }

    #[test]
    fn test_parse_integer_overflow_becomes_float() {
        // This number is larger than i64::MAX, so it should parse as float
        let exprs = parse("9223372036854775808").unwrap();
        assert_eq!(exprs.len(), 1);
        match &exprs[0].value {
            Value::Float(_) => {}
            _ => panic!("expected float for overflow"),
        }
    }

    // ===== Span and Expr tests =====

    #[test]
    fn test_span_display() {
        let span = Span::new(10, 5);
        assert_eq!(format!("{}", span), "line 10, col 5");
    }

    #[test]
    fn test_expr_error_with_span() {
        let expr = Expr::new(Value::Int(42), Span::new(3, 7));
        let error = expr.error("something went wrong");
        assert!(error.contains("something went wrong"));
        assert!(error.contains("line 3"));
        assert!(error.contains("col 7"));
    }

    #[test]
    fn test_expr_error_without_span() {
        let expr = Expr::runtime(Value::Int(42));
        let error = expr.error("something went wrong");
        assert_eq!(error, "something went wrong");
    }

    #[test]
    fn test_expr_runtime_has_no_span() {
        let expr = Expr::runtime(Value::Nil);
        assert!(expr.span.is_none());
    }
}
