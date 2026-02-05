// JSONata expression parser
// Mirrors parser.js from the reference implementation

use crate::ast::{AstNode, BinaryOp, UnaryOp, PathStep, Stage};
use thiserror::Error;

/// Parser errors
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),

    #[error("Unexpected end of expression")]
    UnexpectedEnd,

    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),

    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    #[error("Unclosed string literal")]
    UnclosedString,

    #[error("Invalid escape sequence: {0}")]
    InvalidEscape(String),

    #[error("Unclosed comment")]
    UnclosedComment,

    #[error("Unclosed backtick name")]
    UnclosedBacktick,

    #[error("Expected {expected}, found {found}")]
    Expected { expected: String, found: String },
}

/// Token types for the lexer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    String(String),
    Number(f64),
    True,
    False,
    Null,
    Undefined, // The `undefined` keyword
    Regex { pattern: String, flags: String }, // /pattern/flags

    // Identifiers and operators
    Identifier(String),
    Variable(String),
    ParentVariable(String), // $$ variables
    Function, // function keyword

    // Operators
    Plus,
    Minus,
    Star,
    StarStar, // **
    Slash,
    Percent,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
    In,
    Ampersand,
    Dot,
    DotDot,
    Question,
    QuestionQuestion, // ??
    QuestionColon, // ?:
    Colon,
    ColonEqual, // :=
    TildeArrow, // ~>

    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Semicolon,
    Caret, // ^ sort operator
    Pipe,  // | transform operator

    // Special
    Hash,  // # index binding operator
    Eof,
}

/// Lexer for tokenizing JSONata expressions
pub struct Lexer {
    input: Vec<char>,
    position: usize,
    last_token: Option<Token>,
}

impl Lexer {
    pub fn new(input: String) -> Self {
        Lexer {
            input: input.chars().collect(),
            position: 0,
            last_token: None,
        }
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.position).copied()
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.input.get(self.position + offset).copied()
    }

    fn advance(&mut self) {
        if self.position < self.input.len() {
            self.position += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while self.current().is_some_and(|ch| ch.is_whitespace()) {
            self.advance();
        }
    }

    fn skip_comment(&mut self) -> Result<(), ParserError> {
        // We're at '/', check if next is '*'
        if self.peek(1) == Some('*') {
            self.advance(); // skip '/'
            self.advance(); // skip '*'

            // Find closing */
            loop {
                match self.current() {
                    None => return Err(ParserError::UnclosedComment),
                    Some('*') if self.peek(1) == Some('/') => {
                        self.advance(); // skip '*'
                        self.advance(); // skip '/'
                        break;
                    }
                    Some(_) => self.advance(),
                }
            }
        }
        Ok(())
    }

    fn read_string(&mut self, quote_char: char) -> Result<String, ParserError> {
        let mut result = String::new();
        self.advance(); // skip opening quote

        loop {
            match self.current() {
                None => return Err(ParserError::UnclosedString),
                Some(ch) if ch == quote_char => {
                    self.advance(); // skip closing quote
                    return Ok(result);
                }
                Some('\\') => {
                    self.advance();
                    match self.current() {
                        None => return Err(ParserError::UnclosedString),
                        Some('"') => result.push('"'),
                        Some('\\') => result.push('\\'),
                        Some('/') => result.push('/'),
                        Some('b') => result.push('\u{0008}'),
                        Some('f') => result.push('\u{000C}'),
                        Some('n') => result.push('\n'),
                        Some('r') => result.push('\r'),
                        Some('t') => result.push('\t'),
                        Some('u') => {
                            // Unicode escape sequence \uXXXX
                            self.advance();
                            let mut hex = String::new();
                            for _ in 0..4 {
                                match self.current() {
                                    Some(h) if h.is_ascii_hexdigit() => {
                                        hex.push(h);
                                        self.advance();
                                    }
                                    _ => {
                                        return Err(ParserError::InvalidEscape(format!(
                                            "\\u{}",
                                            hex
                                        )))
                                    }
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16).unwrap();
                            if (0xD800..=0xDBFF).contains(&code) {
                                // High surrogate - expect \uXXXX low surrogate to follow
                                if self.current() == Some('\\') {
                                    self.advance();
                                    if self.current() == Some('u') {
                                        self.advance();
                                        let mut low_hex = String::new();
                                        for _ in 0..4 {
                                            match self.current() {
                                                Some(h) if h.is_ascii_hexdigit() => {
                                                    low_hex.push(h);
                                                    self.advance();
                                                }
                                                _ => {
                                                    return Err(ParserError::InvalidEscape(
                                                        format!("\\u{}", low_hex),
                                                    ))
                                                }
                                            }
                                        }
                                        let low = u32::from_str_radix(&low_hex, 16).unwrap();
                                        if (0xDC00..=0xDFFF).contains(&low) {
                                            let cp = 0x10000 + (code - 0xD800) * 0x400 + (low - 0xDC00);
                                            if let Some(ch) = char::from_u32(cp) {
                                                result.push(ch);
                                            } else {
                                                return Err(ParserError::InvalidEscape(
                                                    format!("\\u{}\\u{}", hex, low_hex),
                                                ));
                                            }
                                        } else {
                                            return Err(ParserError::InvalidEscape(
                                                format!("\\u{}\\u{}", hex, low_hex),
                                            ));
                                        }
                                    } else {
                                        return Err(ParserError::InvalidEscape(
                                            format!("\\u{}", hex),
                                        ));
                                    }
                                } else {
                                    return Err(ParserError::InvalidEscape(
                                        format!("\\u{}", hex),
                                    ));
                                }
                            } else if let Some(ch) = char::from_u32(code) {
                                result.push(ch);
                            } else {
                                return Err(ParserError::InvalidEscape(format!("\\u{}", hex)));
                            }
                            continue; // Don't advance again
                        }
                        Some(ch) => {
                            return Err(ParserError::InvalidEscape(format!("\\{}", ch)))
                        }
                    }
                    self.advance();
                }
                Some(ch) => {
                    result.push(ch);
                    self.advance();
                }
            }
        }
    }

    fn read_number(&mut self) -> Result<f64, ParserError> {
        let start = self.position;

        // Integer part (no minus sign - negation is handled as unary operator)
        if self.current() == Some('0') {
            self.advance();
        } else if self.current().is_some_and(|c| c.is_ascii_digit()) {
            while self.current().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        } else {
            return Err(ParserError::InvalidNumber("Expected digit".to_string()));
        }

        // Fractional part
        if self.current() == Some('.') && self.peek(1) != Some('.') {
            // Only consume '.' if next char is not '.', to avoid consuming '..' range operator
            self.advance();
            if !self.current().is_some_and(|c| c.is_ascii_digit()) {
                return Err(ParserError::InvalidNumber(
                    "Expected digit after decimal point".to_string(),
                ));
            }
            while self.current().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        // Exponent part
        if matches!(self.current(), Some('e') | Some('E')) {
            self.advance();
            if matches!(self.current(), Some('+') | Some('-')) {
                self.advance();
            }
            if !self.current().is_some_and(|c| c.is_ascii_digit()) {
                return Err(ParserError::InvalidNumber(
                    "Expected digit in exponent".to_string(),
                ));
            }
            while self.current().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        let num_str: String = self.input[start..self.position].iter().collect();
        let num: f64 = num_str
            .parse()
            .map_err(|_| ParserError::InvalidNumber(num_str.clone()))?;

        // Check for overflow to infinity
        if num.is_infinite() {
            return Err(ParserError::InvalidNumber(
                format!("S0102: Number out of range: {}", num_str)
            ));
        }

        Ok(num)
    }

    fn read_identifier(&mut self) -> String {
        let start = self.position;

        while let Some(ch) = self.current() {
            // Continue if alphanumeric or underscore
            if ch.is_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }

        self.input[start..self.position].iter().collect()
    }

    fn read_backtick_name(&mut self) -> Result<String, ParserError> {
        self.advance(); // skip opening backtick
        let start = self.position;

        while let Some(ch) = self.current() {
            if ch == '`' {
                let name: String = self.input[start..self.position].iter().collect();
                self.advance(); // skip closing backtick
                return Ok(name);
            }
            self.advance();
        }

        Err(ParserError::UnclosedBacktick)
    }

    fn read_regex(&mut self) -> Result<Token, ParserError> {
        self.advance(); // skip opening /

        // Read pattern
        let mut pattern = String::new();
        let mut escaped = false;

        loop {
            match self.current() {
                None => return Err(ParserError::UnclosedString),
                Some('/') if !escaped => {
                    self.advance(); // skip closing /
                    break;
                }
                Some('\\') if !escaped => {
                    escaped = true;
                    pattern.push('\\');
                    self.advance();
                }
                Some(ch) => {
                    escaped = false;
                    pattern.push(ch);
                    self.advance();
                }
            }
        }

        // Read flags (optional)
        let mut flags = String::new();
        while let Some(ch) = self.current() {
            if ch.is_alphabetic() {
                flags.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        Ok(Token::Regex { pattern, flags })
    }

    fn emit_token(&mut self, token: Token) -> Result<Token, ParserError> {
        self.last_token = Some(token.clone());
        Ok(token)
    }

    pub fn next_token(&mut self) -> Result<Token, ParserError> {
        loop {
            self.skip_whitespace();

            match self.current() {
                None => return Ok(Token::Eof),

                // Comments
                Some('/') if self.peek(1) == Some('*') => {
                    self.skip_comment()?;
                    continue; // Skip whitespace again after comment
                }

                // String literals
                Some('"') => {
                    let s = self.read_string('"')?;
                    return self.emit_token(Token::String(s));
                }
                Some('\'') => {
                    let s = self.read_string('\'')?;
                    return self.emit_token(Token::String(s));
                }

                // Backtick names
                Some('`') => {
                    let name = self.read_backtick_name()?;
                    return self.emit_token(Token::Identifier(name));
                }

                // Numbers (positive only - negation handled as unary operator)
                Some(ch) if ch.is_ascii_digit() => {
                    let num = self.read_number()?;
                    return self.emit_token(Token::Number(num));
                }

                // Variables (start with $)
                Some('$') if self.peek(1) == Some('$') => {
                    // $$ - parent variable
                    self.advance(); // skip first $
                    self.advance(); // skip second $
                    let name = self.read_identifier();
                    return self.emit_token(Token::ParentVariable(name));
                }
                Some('$') => {
                    self.advance();
                    let name = self.read_identifier();
                    return self.emit_token(Token::Variable(name));
                }

                // Two-character operators
                Some('.') if self.peek(1) == Some('.') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::DotDot);
                }
                Some(':') if self.peek(1) == Some('=') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::ColonEqual);
                }
                Some('!') if self.peek(1) == Some('=') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::NotEqual);
                }
                Some('>') if self.peek(1) == Some('=') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::GreaterThanOrEqual);
                }
                Some('<') if self.peek(1) == Some('=') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::LessThanOrEqual);
                }
                Some('~') if self.peek(1) == Some('>') => {
                    self.advance();
                    self.advance();
                    return self.emit_token(Token::TildeArrow);
                }

                // Single-character operators and delimiters
                Some('(') => {
                    self.advance();
                    return Ok(Token::LeftParen);
                }
                Some(')') => {
                    self.advance();
                    return self.emit_token(Token::RightParen);
                }
                Some('[') => {
                    self.advance();
                    return Ok(Token::LeftBracket);
                }
                Some(']') => {
                    self.advance();
                    return self.emit_token(Token::RightBracket);
                }
                Some('{') => {
                    self.advance();
                    return Ok(Token::LeftBrace);
                }
                Some('}') => {
                    self.advance();
                    return self.emit_token(Token::RightBrace);
                }
                Some(',') => {
                    self.advance();
                    return self.emit_token(Token::Comma);
                }
                Some(';') => {
                    self.advance();
                    return self.emit_token(Token::Semicolon);
                }
                Some(':') => {
                    self.advance();
                    return Ok(Token::Colon);
                }
                Some('?') if self.peek(1) == Some('?') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::QuestionQuestion);
                }
                Some('?') if self.peek(1) == Some(':') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::QuestionColon);
                }
                Some('?') => {
                    self.advance();
                    return Ok(Token::Question);
                }
                Some('λ') => {
                    // Lambda symbol (alternative to "function" keyword)
                    self.advance();
                    return Ok(Token::Function);
                }
                Some('.') => {
                    self.advance();
                    return Ok(Token::Dot);
                }
                Some('+') => {
                    self.advance();
                    return Ok(Token::Plus);
                }
                Some('-') => {
                    self.advance();
                    return Ok(Token::Minus);
                }
                Some('*') if self.peek(1) == Some('*') => {
                    self.advance();
                    self.advance();
                    return Ok(Token::StarStar);
                }
                Some('*') => {
                    self.advance();
                    return Ok(Token::Star);
                }
                Some('/') => {
                    // Determine if this is a regex literal or division operator
                    // Regex literals can appear after:
                    // - Start of expression (last_token is None)
                    // - Operators: (, [, {, ,, ;, :, =, !=, <, >, <=, >=, +, -, *, %, &, |, ~, !, ?
                    // - Keywords: and, or, in, function
                    // Division operator appears after:
                    // - Values: ), ], }, identifiers, variables, numbers, strings, etc.

                    let is_regex = match &self.last_token {
                        None => true, // Start of expression
                        Some(Token::LeftParen) | Some(Token::LeftBracket) | Some(Token::LeftBrace) => true,
                        Some(Token::Comma) | Some(Token::Semicolon) | Some(Token::Colon) => true,
                        Some(Token::Equal) | Some(Token::NotEqual) => true,
                        Some(Token::LessThan) | Some(Token::LessThanOrEqual) => true,
                        Some(Token::GreaterThan) | Some(Token::GreaterThanOrEqual) => true,
                        Some(Token::Plus) | Some(Token::Minus) | Some(Token::Star) | Some(Token::Percent) => true,
                        Some(Token::Ampersand) | Some(Token::Question) | Some(Token::TildeArrow) => true,
                        Some(Token::ColonEqual) | Some(Token::QuestionQuestion) | Some(Token::QuestionColon) => true,
                        Some(Token::And) | Some(Token::Or) | Some(Token::In) => true,
                        Some(Token::Function) => true,
                        Some(Token::Identifier(s)) if s == "and" || s == "or" || s == "in" => true,
                        _ => false, // After values, treat as division
                    };

                    if is_regex {
                        let tok = self.read_regex()?;
                        return self.emit_token(tok);
                    } else {
                        self.advance();
                        return self.emit_token(Token::Slash);
                    }
                }
                Some('%') => {
                    self.advance();
                    return Ok(Token::Percent);
                }
                Some('^') => {
                    self.advance();
                    return Ok(Token::Caret);
                }
                Some('#') => {
                    self.advance();
                    return Ok(Token::Hash);
                }
                Some('=') => {
                    self.advance();
                    return Ok(Token::Equal);
                }
                Some('<') => {
                    self.advance();
                    return Ok(Token::LessThan);
                }
                Some('>') => {
                    self.advance();
                    return Ok(Token::GreaterThan);
                }
                Some('&') => {
                    self.advance();
                    return Ok(Token::Ampersand);
                }
                Some('|') => {
                    self.advance();
                    return Ok(Token::Pipe);
                }

                // Identifiers and keywords
                Some(ch) if ch.is_alphabetic() || ch == '_' => {
                    let ident = self.read_identifier();
                    let tok = match ident.as_str() {
                        "true" => Token::True,
                        "false" => Token::False,
                        "null" => Token::Null,
                        "undefined" => Token::Undefined,
                        "function" => Token::Function,
                        // "and", "or", "in" are now contextual keywords (handled in parser)
                        _ => Token::Identifier(ident),
                    };
                    return self.emit_token(tok);
                }

                Some(ch) => {
                    return Err(ParserError::UnexpectedToken(ch.to_string()));
                }
            }
        }
    }
}

/// Parser for JSONata expressions using Pratt parsing
pub struct Parser {
    lexer: Lexer,
    current_token: Token,
}

impl Parser {
    pub fn new(input: String) -> Result<Self, ParserError> {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token()?;
        Ok(Parser {
            lexer,
            current_token,
        })
    }

    fn advance(&mut self) -> Result<(), ParserError> {
        self.current_token = self.lexer.next_token()?;
        Ok(())
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParserError> {
        if std::mem::discriminant(&self.current_token) == std::mem::discriminant(&expected) {
            self.advance()?;
            Ok(())
        } else {
            Err(ParserError::Expected {
                expected: format!("{:?}", expected),
                found: format!("{:?}", self.current_token),
            })
        }
    }

    /// Get the binding power (precedence) for the current token
    fn binding_power(&self, token: &Token) -> Option<(u8, u8)> {
        // Returns (left_bp, right_bp) for left and right binding power
        // Higher numbers = higher precedence
        match token {
            // Contextual keywords (treated as operators when in infix position)
            Token::Identifier(name) if name == "or" => Some((25, 26)),
            Token::Identifier(name) if name == "and" => Some((30, 31)),
            Token::Identifier(name) if name == "in" => Some((40, 41)),
            // Regular operators
            Token::Or => Some((25, 26)),
            Token::And => Some((30, 31)),
            Token::Equal | Token::NotEqual | Token::LessThan | Token::LessThanOrEqual
            | Token::GreaterThan | Token::GreaterThanOrEqual | Token::In => Some((40, 41)),
            Token::Ampersand => Some((50, 51)),
            Token::Plus | Token::Minus => Some((50, 51)),
            Token::Star | Token::Slash | Token::Percent => Some((60, 61)),
            Token::Dot => Some((75, 85)), // Right bp is higher to prevent consuming postfix operators
            Token::LeftBracket => Some((80, 81)),
            Token::LeftParen => Some((80, 81)),
            Token::LeftBrace => Some((80, 81)), // Object constructor as postfix
            Token::Caret => Some((80, 81)), // Sort operator as postfix
            Token::Hash => Some((80, 81)),  // Index binding operator as postfix
            Token::Question => Some((20, 21)),
            Token::QuestionQuestion => Some((15, 16)), // Coalescing operator
            Token::QuestionColon => Some((15, 16)), // Default operator
            Token::DotDot => Some((20, 21)),
            Token::ColonEqual => Some((10, 9)), // Right associative
            Token::TildeArrow => Some((70, 71)), // Chain/pipe operator
            _ => None,
        }
    }

    /// Parse a function signature: <param-types:return-type>
    fn parse_signature(&mut self) -> Result<String, ParserError> {
        // Expect <
        if self.current_token != Token::LessThan {
            return Err(ParserError::Expected {
                expected: "<".to_string(),
                found: format!("{:?}", self.current_token),
            });
        }

        // Build signature string by collecting characters until we find >
        let mut signature = String::from("<");
        self.advance()?; // skip <

        // Collect all characters until we find the closing >
        // This is a bit tricky because we need to handle nested <> for array types like a<s>
        let mut depth = 1;

        while depth > 0 && self.current_token != Token::Eof {
            match &self.current_token {
                Token::LessThan => {
                    signature.push('<');
                    depth += 1;
                    self.advance()?;
                }
                Token::GreaterThan => {
                    depth -= 1;
                    if depth > 0 {
                        signature.push('>');
                    }
                    self.advance()?;
                }
                Token::Minus => {
                    signature.push('-');
                    self.advance()?;
                }
                Token::Colon => {
                    signature.push(':');
                    self.advance()?;
                }
                Token::Question => {
                    signature.push('?');
                    self.advance()?;
                }
                Token::QuestionColon => {
                    // ?:  gets tokenized as QuestionColon in signatures like s?:s
                    signature.push('?');
                    signature.push(':');
                    self.advance()?;
                }
                Token::Identifier(s) => {
                    signature.push_str(s);
                    self.advance()?;
                }
                Token::LeftParen | Token::RightParen => {
                    // Handle parentheses for union types like (ns)
                    let c = if self.current_token == Token::LeftParen { '(' } else { ')' };
                    signature.push(c);
                    self.advance()?;
                }
                _ => {
                    return Err(ParserError::UnexpectedToken(format!(
                        "Unexpected token in signature: {:?}",
                        self.current_token
                    )));
                }
            }
        }

        signature.push('>');
        Ok(signature)
    }

    /// Parse a primary expression (literals, identifiers, variables, grouping)
    fn parse_primary(&mut self) -> Result<AstNode, ParserError> {
        match &self.current_token {
            Token::String(s) => {
                let value = s.clone();
                self.advance()?;
                Ok(AstNode::String(value))
            }
            Token::Number(n) => {
                let value = *n;
                self.advance()?;
                Ok(AstNode::Number(value))
            }
            Token::True => {
                self.advance()?;
                Ok(AstNode::Boolean(true))
            }
            Token::False => {
                self.advance()?;
                Ok(AstNode::Boolean(false))
            }
            Token::Null => {
                self.advance()?;
                Ok(AstNode::Null)
            }
            Token::Undefined => {
                self.advance()?;
                Ok(AstNode::Undefined)
            }
            Token::Regex { pattern, flags } => {
                let pat = pattern.clone();
                let flg = flags.clone();
                self.advance()?;
                Ok(AstNode::Regex { pattern: pat, flags: flg })
            }
            Token::Identifier(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(AstNode::Path {
                    steps: vec![PathStep::new(AstNode::Name(name))],
                })
            }
            Token::Variable(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(AstNode::Variable(name))
            }
            Token::ParentVariable(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(AstNode::ParentVariable(name))
            }
            Token::LeftParen => {
                self.advance()?; // skip '('

                // Check for empty parentheses () which means undefined
                if self.current_token == Token::RightParen {
                    self.advance()?;
                    return Ok(AstNode::Undefined);
                }

                // Parse block expressions (separated by semicolons)
                // NOTE: Parentheses ALWAYS create a block in JSONata, even with a single expression.
                // This is important for variable scoping - ( $x := value ) creates a new scope.
                let mut expressions = vec![self.parse_expression(0)?];

                while self.current_token == Token::Semicolon {
                    self.advance()?;
                    if self.current_token == Token::RightParen {
                        break;
                    }
                    expressions.push(self.parse_expression(0)?);
                }

                self.expect(Token::RightParen)?;

                // Always create a block, matching JavaScript implementation
                Ok(AstNode::Block(expressions))
            }
            Token::LeftBracket => {
                self.advance()?; // skip '['

                let mut elements = Vec::new();

                if self.current_token != Token::RightBracket {
                    loop {
                        let element = self.parse_expression(0)?;
                        elements.push(element);

                        if self.current_token != Token::Comma {
                            break;
                        }
                        self.advance()?;
                    }
                }

                self.expect(Token::RightBracket)?;
                Ok(AstNode::Array(elements))
            }
            Token::LeftBrace => {
                self.advance()?; // skip '{'

                let mut pairs = Vec::new();

                if self.current_token != Token::RightBrace {
                    loop {
                        let key = self.parse_expression(0)?;
                        self.expect(Token::Colon)?;
                        let value = self.parse_expression(0)?;
                        pairs.push((key, value));

                        if self.current_token != Token::Comma {
                            break;
                        }
                        self.advance()?;
                    }
                }

                self.expect(Token::RightBrace)?;
                Ok(AstNode::Object(pairs))
            }
            Token::Pipe => {
                // Transform operator: |location|update[,delete]|
                self.advance()?; // skip first '|'

                // Parse location expression
                let location = self.parse_expression(0)?;

                // Expect second '|'
                self.expect(Token::Pipe)?;

                // Parse update expression (object constructor)
                let update = self.parse_expression(0)?;

                // Check for optional delete part
                let delete = if self.current_token == Token::Comma {
                    self.advance()?; // skip comma
                    Some(Box::new(self.parse_expression(0)?))
                } else {
                    None
                };

                // Expect final '|'
                self.expect(Token::Pipe)?;

                Ok(AstNode::Transform {
                    location: Box::new(location),
                    update: Box::new(update),
                    delete,
                })
            }
            Token::Minus => {
                self.advance()?;
                let operand = self.parse_expression(70)?; // High precedence for unary
                Ok(AstNode::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(operand),
                })
            }
            Token::Star => {
                // Wildcard operator in primary position
                self.advance()?;
                Ok(AstNode::Wildcard)
            }
            Token::StarStar => {
                // Descendant operator in primary position
                self.advance()?;
                Ok(AstNode::Descendant)
            }
            Token::Function => {
                // Parse lambda: function($param1, $param2, ...) { body }
                self.advance()?; // skip 'function'
                self.expect(Token::LeftParen)?;

                // Parse parameters
                let mut params = Vec::new();
                if self.current_token != Token::RightParen {
                    loop {
                        match &self.current_token {
                            Token::Variable(name) => {
                                params.push(name.clone());
                                self.advance()?;
                            }
                            _ => {
                                return Err(ParserError::Expected {
                                    expected: "parameter name".to_string(),
                                    found: format!("{:?}", self.current_token),
                                })
                            }
                        }

                        if self.current_token != Token::Comma {
                            break;
                        }
                        self.advance()?; // skip comma
                    }
                }

                self.expect(Token::RightParen)?;

                // Check for optional signature: <type-type:returntype>
                let signature = if self.current_token == Token::LessThan {
                    Some(self.parse_signature()?)
                } else {
                    None
                };

                self.expect(Token::LeftBrace)?;

                // Parse body
                let body = self.parse_expression(0)?;

                self.expect(Token::RightBrace)?;

                // Apply tail call optimization to the body
                let (optimized_body, is_thunk) = Self::tail_call_optimize(body);

                Ok(AstNode::Lambda {
                    params,
                    body: Box::new(optimized_body),
                    signature,
                    thunk: is_thunk,
                })
            }
            _ => Err(ParserError::UnexpectedToken(format!(
                "{:?}",
                self.current_token
            ))),
        }
    }

    /// Parse an expression with Pratt parsing
    fn parse_expression(&mut self, min_bp: u8) -> Result<AstNode, ParserError> {
        let mut lhs = self.parse_primary()?;

        loop {
            // Check for end of expression
            if matches!(
                self.current_token,
                Token::Eof
                    | Token::RightParen
                    | Token::RightBracket
                    | Token::RightBrace
                    | Token::Comma
                    | Token::Semicolon
                    | Token::Colon
            ) {
                break;
            }

            // Get binding power for current operator
            let (left_bp, right_bp) = match self.binding_power(&self.current_token) {
                Some(bp) => bp,
                None => break,
            };

            if left_bp < min_bp {
                break;
            }

            // Handle infix operators
            match &self.current_token {
                Token::Dot => {
                    self.advance()?;

                    // Check for .[expr] syntax (array grouping)
                    if self.current_token == Token::LeftBracket {
                        self.advance()?;

                        // Parse the array elements
                        let mut elements = Vec::new();
                        if self.current_token != Token::RightBracket {
                            loop {
                                elements.push(self.parse_expression(0)?);
                                if self.current_token != Token::Comma {
                                    break;
                                }
                                self.advance()?;
                            }
                        }

                        self.expect(Token::RightBracket)?;

                        // Create ArrayGroup node as a path step
                        let mut steps = match lhs {
                            AstNode::Path { steps } => steps,
                            _ => vec![PathStep::new(lhs)],
                        };

                        steps.push(PathStep::new(AstNode::ArrayGroup(elements)));
                        lhs = AstNode::Path { steps };
                    } else if self.current_token == Token::LeftParen {
                        // Check for .(expr) syntax (function application)
                        self.advance()?;

                        // Parse the expression(s) to apply - may be block with semicolons
                        let mut expressions = vec![self.parse_expression(0)?];

                        while self.current_token == Token::Semicolon {
                            self.advance()?;
                            if self.current_token == Token::RightParen {
                                break;
                            }
                            expressions.push(self.parse_expression(0)?);
                        }

                        self.expect(Token::RightParen)?;

                        // Wrap in Block if multiple expressions, otherwise use single expression
                        let expr = if expressions.len() == 1 {
                            expressions.into_iter().next().unwrap()
                        } else {
                            AstNode::Block(expressions)
                        };

                        // Create FunctionApplication node as a path step
                        let mut steps = match lhs {
                            AstNode::Path { steps } => steps,
                            _ => vec![PathStep::new(lhs)],
                        };

                        steps.push(PathStep::new(AstNode::FunctionApplication(Box::new(expr))));
                        lhs = AstNode::Path { steps };
                    } else {
                        // Normal dot path
                        let rhs = self.parse_expression(right_bp)?;

                        // Flatten path expressions
                        let mut steps = match lhs {
                            AstNode::Path { steps } => steps,
                            // Convert string literals to field names when used as first step in path
                            // e.g., "foo".bar should behave like foo.bar
                            AstNode::String(field_name) => vec![PathStep::new(AstNode::Name(field_name))],
                            _ => vec![PathStep::new(lhs)],
                        };

                        // S0213: The literal value cannot be used as a step within a path expression
                        // Numbers, booleans (true/false), and null cannot be path steps
                        match &rhs {
                            AstNode::Number(n) => {
                                return Err(ParserError::InvalidSyntax(
                                    format!("S0213: The literal value {} cannot be used as a step within a path expression", n),
                                ));
                            }
                            AstNode::Boolean(b) => {
                                return Err(ParserError::InvalidSyntax(
                                    format!("S0213: The literal value {} cannot be used as a step within a path expression", b),
                                ));
                            }
                            AstNode::Null => {
                                return Err(ParserError::InvalidSyntax(
                                    "S0213: The literal value null cannot be used as a step within a path expression".to_string(),
                                ));
                            }
                            _ => {}
                        }

                        match rhs {
                            AstNode::Path { steps: mut rhs_steps } => {
                                steps.append(&mut rhs_steps);
                            }
                            // Convert string literals to field names when they appear after a dot
                            // e.g., $."Field.Name" should access a property named "Field.Name"
                            AstNode::String(field_name) => {
                                steps.push(PathStep::new(AstNode::Name(field_name)));
                            }
                            _ => steps.push(PathStep::new(rhs)),
                        }

                        // Check for following predicates and attach as stages to the last step
                        // This implements JSONata semantics where foo.bar[0] has [0] apply during extraction
                        while self.current_token == Token::LeftBracket {
                            self.advance()?;

                            let predicate_expr = if self.current_token == Token::RightBracket {
                                // Empty brackets []
                                Box::new(AstNode::Boolean(true))
                            } else {
                                // Normal predicate expression
                                let pred = self.parse_expression(0)?;
                                Box::new(pred)
                            };

                            self.expect(Token::RightBracket)?;

                            // Attach predicate as stage to the last step
                            if let Some(last_step) = steps.last_mut() {
                                last_step.stages.push(Stage::Filter(predicate_expr));
                            }
                        }

                        lhs = AstNode::Path { steps };
                    }
                }
                Token::LeftBracket => {
                    // S0209: A predicate cannot follow a grouping expression in a step
                    // Check if lhs is an ObjectTransform (grouping expression)
                    if matches!(lhs, AstNode::ObjectTransform { .. }) {
                        return Err(ParserError::InvalidSyntax(
                            "S0209: A predicate cannot follow a grouping expression in a step".to_string(),
                        ));
                    }

                    self.advance()?;

                    // Predicates in postfix position are always separate steps
                    // Predicates as stages are only attached during DOT operator parsing
                    if self.current_token == Token::RightBracket {
                        // Empty brackets []
                        self.advance()?;

                        let mut steps = match lhs {
                            AstNode::Path { steps } => steps,
                            _ => vec![PathStep::new(lhs)],
                        };

                        steps.push(PathStep::new(AstNode::Predicate(Box::new(AstNode::Boolean(true)))));
                        lhs = AstNode::Path { steps };
                    } else {
                        // Normal predicate
                        let predicate = self.parse_expression(0)?;
                        self.expect(Token::RightBracket)?;

                        let mut steps = match lhs {
                            AstNode::Path { steps } => steps,
                            _ => vec![PathStep::new(lhs)],
                        };

                        steps.push(PathStep::new(AstNode::Predicate(Box::new(predicate))));
                        lhs = AstNode::Path { steps };
                    }
                }
                Token::LeftParen => {
                    self.advance()?;

                    let mut args = Vec::new();

                    if self.current_token != Token::RightParen {
                        loop {
                            // Check for ? placeholder (partial application)
                            if self.current_token == Token::Question {
                                args.push(AstNode::Placeholder);
                                self.advance()?;
                            } else {
                                args.push(self.parse_expression(0)?);
                            }

                            if self.current_token != Token::Comma {
                                break;
                            }
                            self.advance()?;
                        }
                    }

                    self.expect(Token::RightParen)?;

                    // Check if lhs is a lambda or callable expression
                    match lhs {
                        // Direct invocations: lambda(args), block(args), chained calls, function result calls
                        AstNode::Lambda { .. }
                        | AstNode::Block(_)
                        | AstNode::Call { .. }
                        | AstNode::Function { .. } => {
                            lhs = AstNode::Call {
                                procedure: Box::new(lhs),
                                args,
                            };
                        }
                        ref other_lhs => {
                            // Extract function name from lhs
                            match other_lhs {
                                // Handle bare function names: uppercase()
                                AstNode::Path { steps } if steps.len() == 1 => {
                                    let name = match &steps[0].node {
                                        AstNode::Name(s) => s.clone(),
                                        _ => return Err(ParserError::InvalidSyntax(
                                            "Invalid function name".to_string()
                                        )),
                                    };
                                    lhs = AstNode::Function { name, args, is_builtin: false };
                                }
                                // Handle path ending with $function: foo.bar.$lowercase(args)
                                AstNode::Path { steps } if steps.len() > 1 => {
                                    let last_step = &steps[steps.len() - 1].node;

                                    // Check if last step is a Variable (function reference)
                                    if let AstNode::Variable(func_name) = last_step {
                                        // Extract all but the last step as the path context
                                        let mut context_steps = steps.clone();
                                        context_steps.pop();

                                        // Create function call
                                        let func_call = AstNode::Function {
                                            name: func_name.clone(),
                                            args: args.clone(),
                                            is_builtin: true, // Variable means $ prefix
                                        };

                                        // Append function application to the path
                                        context_steps.push(PathStep::new(AstNode::FunctionApplication(Box::new(func_call))));

                                        lhs = AstNode::Path { steps: context_steps };
                                    }
                                    // Check if last step is a Lambda (inline function in path)
                                    else if let AstNode::Lambda { params, body, signature, thunk } = last_step {
                                        // Extract all but the last step as the path context
                                        let mut context_steps = steps.clone();
                                        context_steps.pop();

                                        // In path context, determine if we need to prepend $
                                        // - If fewer args than params, prepend $ (context value) as first arg
                                        // - If args == params, use args as-is
                                        let full_args = if args.len() < params.len() {
                                            let mut new_args = vec![AstNode::Variable("$".to_string())];
                                            new_args.extend(args.clone());
                                            new_args
                                        } else {
                                            args.clone()
                                        };

                                        // Create a lambda invocation block
                                        // ($__path_lambda := lambda; $__path_lambda(args...))
                                        let lambda_invocation = AstNode::Block(vec![
                                            AstNode::Binary {
                                                op: crate::ast::BinaryOp::ColonEqual,
                                                lhs: Box::new(AstNode::Variable("__path_lambda__".to_string())),
                                                rhs: Box::new(AstNode::Lambda {
                                                    params: params.clone(),
                                                    body: body.clone(),
                                                    signature: signature.clone(),
                                                    thunk: *thunk,
                                                }),
                                            },
                                            AstNode::Function {
                                                name: "__path_lambda__".to_string(),
                                                args: full_args,
                                                is_builtin: true,
                                            },
                                        ]);

                                        // Append as function application to the path
                                        context_steps.push(PathStep::new(AstNode::FunctionApplication(Box::new(lambda_invocation))));

                                        lhs = AstNode::Path { steps: context_steps };
                                    } else {
                                        return Err(ParserError::InvalidSyntax(
                                            "Invalid function call".to_string()
                                        ));
                                    }
                                }
                                // Handle $-prefixed function names: $uppercase()
                                AstNode::Variable(name) => {
                                    lhs = AstNode::Function { name: name.clone(), args, is_builtin: true };
                                }
                                _ => return Err(ParserError::InvalidSyntax(
                                    "Invalid function call".to_string()
                                )),
                            };
                        }
                    }
                }
                Token::Question => {
                    self.advance()?;
                    let then_branch = self.parse_expression(0)?;

                    let else_branch = if self.current_token == Token::Colon {
                        self.advance()?;
                        // Use 0 for right-associativity: a ? b : c ? d : e parses as a ? b : (c ? d : e)
                        Some(Box::new(self.parse_expression(0)?))
                    } else {
                        None
                    };

                    lhs = AstNode::Conditional {
                        condition: Box::new(lhs),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    };
                }
                Token::LeftBrace => {
                    // S0210: Each step can only have one grouping expression
                    // Check if lhs is already an ObjectTransform
                    if matches!(lhs, AstNode::ObjectTransform { .. }) {
                        return Err(ParserError::InvalidSyntax(
                            "S0210: Each step can only have one grouping expression".to_string(),
                        ));
                    }

                    // Object constructor as postfix: expr{key: value}
                    self.advance()?; // skip '{'

                    let mut pairs = Vec::new();

                    if self.current_token != Token::RightBrace {
                        loop {
                            // Parse key expression - parse_expression handles identifiers correctly
                            let key = self.parse_expression(0)?;

                            self.expect(Token::Colon)?;

                            // Parse value expression - can be any expression including paths
                            let value = self.parse_expression(0)?;

                            pairs.push((key, value));

                            if self.current_token != Token::Comma {
                                break;
                            }
                            self.advance()?; // skip comma
                        }
                    }

                    self.expect(Token::RightBrace)?;

                    // Object constructor with input: lhs{k:v} means transform lhs using the object pattern
                    lhs = AstNode::ObjectTransform {
                        input: Box::new(lhs),
                        pattern: pairs,
                    };
                }
                Token::Hash => {
                    // Index binding operator: #$var
                    // Binds the current array index to the specified variable
                    self.advance()?; // skip '#'

                    // Expect a variable name
                    let var_name = match &self.current_token {
                        Token::Variable(name) => name.clone(),
                        _ => {
                            return Err(ParserError::InvalidSyntax(
                                "Expected variable name after #".to_string(),
                            ));
                        }
                    };
                    self.advance()?; // skip variable

                    lhs = AstNode::IndexBind {
                        input: Box::new(lhs),
                        variable: var_name,
                    };
                }
                Token::Caret => {
                    // Sort operator: ^(expr) or ^(<expr) or ^(>expr)
                    self.advance()?; // skip '^'
                    self.expect(Token::LeftParen)?;

                    let mut terms = Vec::new();

                    loop {
                        // Check for optional sort direction prefix
                        let ascending = match &self.current_token {
                            Token::LessThan => {
                                self.advance()?;
                                true
                            }
                            Token::GreaterThan => {
                                self.advance()?;
                                false
                            }
                            _ => true, // Default to ascending
                        };

                        // Parse the sort expression
                        let expr = self.parse_expression(0)?;
                        terms.push((expr, ascending));

                        // Check for more sort terms
                        if self.current_token != Token::Comma {
                            break;
                        }
                        self.advance()?; // skip comma
                    }

                    self.expect(Token::RightParen)?;

                    lhs = AstNode::Sort {
                        input: Box::new(lhs),
                        terms
                    };
                }
                _ => {
                    // Binary operators
                    let op = match &self.current_token {
                        // Contextual keyword operators
                        Token::Identifier(name) if name == "and" => BinaryOp::And,
                        Token::Identifier(name) if name == "or" => BinaryOp::Or,
                        Token::Identifier(name) if name == "in" => BinaryOp::In,
                        // Regular operators
                        Token::Plus => BinaryOp::Add,
                        Token::Minus => BinaryOp::Subtract,
                        Token::Star => BinaryOp::Multiply,
                        Token::Slash => BinaryOp::Divide,
                        Token::Percent => BinaryOp::Modulo,
                        Token::Equal => BinaryOp::Equal,
                        Token::NotEqual => BinaryOp::NotEqual,
                        Token::LessThan => BinaryOp::LessThan,
                        Token::LessThanOrEqual => BinaryOp::LessThanOrEqual,
                        Token::GreaterThan => BinaryOp::GreaterThan,
                        Token::GreaterThanOrEqual => BinaryOp::GreaterThanOrEqual,
                        Token::And => BinaryOp::And,
                        Token::Or => BinaryOp::Or,
                        Token::In => BinaryOp::In,
                        Token::Ampersand => BinaryOp::Concatenate,
                        Token::DotDot => BinaryOp::Range,
                        Token::ColonEqual => BinaryOp::ColonEqual,
                        Token::QuestionQuestion => BinaryOp::Coalesce,
                        Token::QuestionColon => BinaryOp::Default,
                        Token::TildeArrow => BinaryOp::ChainPipe,
                        _ => {
                            return Err(ParserError::UnexpectedToken(format!(
                                "{:?}",
                                self.current_token
                            )))
                        }
                    };

                    self.advance()?;
                    let rhs = self.parse_expression(right_bp)?;

                    lhs = AstNode::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
            }
        }

        Ok(lhs)
    }

    pub fn parse(&mut self) -> Result<AstNode, ParserError> {
        let ast = self.parse_expression(0)?;

        if self.current_token != Token::Eof {
            return Err(ParserError::Expected {
                expected: "end of expression".to_string(),
                found: format!("{:?}", self.current_token),
            });
        }

        Ok(ast)
    }

    /// Analyze an expression for tail call optimization
    /// Returns (optimized_expr, is_thunk) where:
    /// - optimized_expr is the expression (unchanged)
    /// - is_thunk is true if the expression's tail position is a function call
    ///
    /// A tail position is where a function call's result is directly returned:
    /// - The body itself if it's a function call
    /// - Both branches of a conditional at tail position
    /// - The last expression of a block at tail position
    fn tail_call_optimize(expr: AstNode) -> (AstNode, bool) {
        let is_thunk = Self::is_tail_call(&expr);
        (expr, is_thunk)
    }

    /// Check if an expression is in tail call position
    /// Returns true if the expression is a function call (or contains function calls in all tail positions)
    fn is_tail_call(expr: &AstNode) -> bool {
        match expr {
            // Direct function calls are tail calls
            AstNode::Function { .. } => true,
            AstNode::Call { .. } => true,

            // Conditional: both branches must be tail calls (or at least one if only one branch)
            AstNode::Conditional { then_branch, else_branch, .. } => {
                let then_is_tail = Self::is_tail_call(then_branch);
                let else_is_tail = else_branch.as_ref().map_or(false, |e| Self::is_tail_call(e));
                // At least one branch should be a tail call for TCO to be useful
                then_is_tail || else_is_tail
            }

            // Block: last expression is tail position
            AstNode::Block(exprs) => {
                exprs.last().map_or(false, |last| Self::is_tail_call(last))
            }

            // Variable binding with result: the result expression is tail position
            AstNode::Binary { op: BinaryOp::ColonEqual, rhs, .. } => {
                // The rhs (or next expression) could be tail position
                // But typically := is used for assignment within blocks
                // Check if rhs is a block (common pattern)
                Self::is_tail_call(rhs)
            }

            // Anything else is not a tail call
            _ => false,
        }
    }
}

/// Parse a JSONata expression string into an AST
///
/// This is the main entry point for parsing.
pub fn parse(expression: &str) -> Result<AstNode, ParserError> {
    let mut parser = Parser::new(expression.to_string())?;
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Lexer tests
    #[test]
    fn test_lexer_numbers() {
        let mut lexer = Lexer::new("42 3.14 -10 2.5e10 1E-5".to_string());

        assert_eq!(lexer.next_token().unwrap(), Token::Number(42.0));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3.14));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(-10.0));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2.5e10));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1e-5));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_strings() {
        let mut lexer = Lexer::new(r#""hello" 'world' "with\nnewline""#.to_string());

        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String("hello".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String("world".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String("with\nnewline".to_string())
        );
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_string_escapes() {
        let mut lexer = Lexer::new(r#""a\"b\\c\/d""#.to_string());
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String("a\"b\\c/d".to_string())
        );
    }

    #[test]
    fn test_lexer_keywords() {
        let mut lexer = Lexer::new("true false null and or in".to_string());

        assert_eq!(lexer.next_token().unwrap(), Token::True);
        assert_eq!(lexer.next_token().unwrap(), Token::False);
        assert_eq!(lexer.next_token().unwrap(), Token::Null);
        assert_eq!(lexer.next_token().unwrap(), Token::And);
        assert_eq!(lexer.next_token().unwrap(), Token::Or);
        assert_eq!(lexer.next_token().unwrap(), Token::In);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_identifiers() {
        let mut lexer = Lexer::new("foo bar_baz test123".to_string());

        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("foo".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("bar_baz".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("test123".to_string())
        );
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_variables() {
        let mut lexer = Lexer::new("$var $foo_bar".to_string());

        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Variable("var".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Variable("foo_bar".to_string())
        );
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_operators() {
        let mut lexer = Lexer::new("+ - * / % = != < <= > >= & . .. := ".to_string());

        assert_eq!(lexer.next_token().unwrap(), Token::Plus);
        assert_eq!(lexer.next_token().unwrap(), Token::Minus);
        assert_eq!(lexer.next_token().unwrap(), Token::Star);
        assert_eq!(lexer.next_token().unwrap(), Token::Slash);
        assert_eq!(lexer.next_token().unwrap(), Token::Percent);
        assert_eq!(lexer.next_token().unwrap(), Token::Equal);
        assert_eq!(lexer.next_token().unwrap(), Token::NotEqual);
        assert_eq!(lexer.next_token().unwrap(), Token::LessThan);
        assert_eq!(lexer.next_token().unwrap(), Token::LessThanOrEqual);
        assert_eq!(lexer.next_token().unwrap(), Token::GreaterThan);
        assert_eq!(lexer.next_token().unwrap(), Token::GreaterThanOrEqual);
        assert_eq!(lexer.next_token().unwrap(), Token::Ampersand);
        assert_eq!(lexer.next_token().unwrap(), Token::Dot);
        assert_eq!(lexer.next_token().unwrap(), Token::DotDot);
        assert_eq!(lexer.next_token().unwrap(), Token::ColonEqual);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_delimiters() {
        let mut lexer = Lexer::new("()[]{},:;?".to_string());

        assert_eq!(lexer.next_token().unwrap(), Token::LeftParen);
        assert_eq!(lexer.next_token().unwrap(), Token::RightParen);
        assert_eq!(lexer.next_token().unwrap(), Token::LeftBracket);
        assert_eq!(lexer.next_token().unwrap(), Token::RightBracket);
        assert_eq!(lexer.next_token().unwrap(), Token::LeftBrace);
        assert_eq!(lexer.next_token().unwrap(), Token::RightBrace);
        assert_eq!(lexer.next_token().unwrap(), Token::Comma);
        assert_eq!(lexer.next_token().unwrap(), Token::Colon);
        assert_eq!(lexer.next_token().unwrap(), Token::Semicolon);
        assert_eq!(lexer.next_token().unwrap(), Token::Question);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_empty_brackets() {
        let mut parser = Parser::new("foo[]".to_string()).unwrap();
        let ast = parser.parse().unwrap();

        // Should create a Path with two steps: Name("foo") and Predicate(Boolean(true))
        if let AstNode::Path { steps } = ast {
            assert_eq!(steps.len(), 2);
            assert!(matches!(steps[0].node, AstNode::Name(ref s) if s == "foo"));
            if let AstNode::Predicate(pred) = &steps[1].node {
                assert!(matches!(**pred, AstNode::Boolean(true)));
            } else {
                panic!("Expected Predicate as second step, got {:?}", steps[1].node);
            }
        } else {
            panic!("Expected Path, got {:?}", ast);
        }
    }

    #[test]
    fn test_lexer_comments() {
        let mut lexer = Lexer::new("foo /* comment */ bar".to_string());

        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("foo".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("bar".to_string())
        );
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_lexer_backtick_names() {
        let mut lexer = Lexer::new("`field name` `with-dash`".to_string());

        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("field name".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("with-dash".to_string())
        );
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    // Parser tests
    #[test]
    fn test_parse_number() {
        let ast = parse("42").unwrap();
        assert_eq!(ast, AstNode::Number(42.0));
    }

    #[test]
    fn test_parse_string() {
        let ast = parse(r#""hello""#).unwrap();
        assert_eq!(ast, AstNode::String("hello".to_string()));
    }

    #[test]
    fn test_parse_boolean() {
        let ast = parse("true").unwrap();
        assert_eq!(ast, AstNode::Boolean(true));

        let ast = parse("false").unwrap();
        assert_eq!(ast, AstNode::Boolean(false));
    }

    #[test]
    fn test_parse_null() {
        let ast = parse("null").unwrap();
        assert_eq!(ast, AstNode::Null);
    }

    #[test]
    fn test_parse_variable() {
        let ast = parse("$var").unwrap();
        assert_eq!(ast, AstNode::Variable("var".to_string()));
    }

    #[test]
    fn test_parse_identifier() {
        let ast = parse("foo").unwrap();
        assert_eq!(
            ast,
            AstNode::Path {
                steps: vec![PathStep::new(AstNode::Name("foo".to_string()))]
            }
        );
    }

    #[test]
    fn test_parse_addition() {
        let ast = parse("1 + 2").unwrap();
        match ast {
            AstNode::Binary { op, lhs, rhs } => {
                assert_eq!(op, BinaryOp::Add);
                assert_eq!(*lhs, AstNode::Number(1.0));
                assert_eq!(*rhs, AstNode::Number(2.0));
            }
            _ => panic!("Expected Binary node"),
        }
    }

    #[test]
    fn test_parse_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let ast = parse("1 + 2 * 3").unwrap();
        match ast {
            AstNode::Binary {
                op: BinaryOp::Add,
                lhs,
                rhs,
            } => {
                assert_eq!(*lhs, AstNode::Number(1.0));
                match *rhs {
                    AstNode::Binary {
                        op: BinaryOp::Multiply,
                        lhs,
                        rhs,
                    } => {
                        assert_eq!(*lhs, AstNode::Number(2.0));
                        assert_eq!(*rhs, AstNode::Number(3.0));
                    }
                    _ => panic!("Expected Binary node for multiplication"),
                }
            }
            _ => panic!("Expected Binary node for addition"),
        }
    }

    #[test]
    fn test_parse_parentheses() {
        // (1 + 2) * 3 should parse as (1 + 2) * 3
        let ast = parse("(1 + 2) * 3").unwrap();
        match ast {
            AstNode::Binary {
                op: BinaryOp::Multiply,
                lhs,
                rhs,
            } => {
                match *lhs {
                    AstNode::Binary {
                        op: BinaryOp::Add,
                        lhs,
                        rhs,
                    } => {
                        assert_eq!(*lhs, AstNode::Number(1.0));
                        assert_eq!(*rhs, AstNode::Number(2.0));
                    }
                    _ => panic!("Expected Binary node for addition"),
                }
                assert_eq!(*rhs, AstNode::Number(3.0));
            }
            _ => panic!("Expected Binary node for multiplication"),
        }
    }

    #[test]
    fn test_parse_array() {
        let ast = parse("[1, 2, 3]").unwrap();
        match ast {
            AstNode::Array(elements) => {
                assert_eq!(elements.len(), 3);
                assert_eq!(elements[0], AstNode::Number(1.0));
                assert_eq!(elements[1], AstNode::Number(2.0));
                assert_eq!(elements[2], AstNode::Number(3.0));
            }
            _ => panic!("Expected Array node"),
        }
    }

    #[test]
    fn test_parse_object() {
        let ast = parse(r#"{"a": 1, "b": 2}"#).unwrap();
        match ast {
            AstNode::Object(pairs) => {
                assert_eq!(pairs.len(), 2);
                assert_eq!(pairs[0].0, AstNode::String("a".to_string()));
                assert_eq!(pairs[0].1, AstNode::Number(1.0));
                assert_eq!(pairs[1].0, AstNode::String("b".to_string()));
                assert_eq!(pairs[1].1, AstNode::Number(2.0));
            }
            _ => panic!("Expected Object node"),
        }
    }

    #[test]
    fn test_parse_path() {
        let ast = parse("foo.bar").unwrap();
        match ast {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].node, AstNode::Name("foo".to_string()));
                assert_eq!(steps[1].node, AstNode::Name("bar".to_string()));
            }
            _ => panic!("Expected Path node"),
        }
    }

    #[test]
    fn test_parse_function_call() {
        let ast = parse("sum(1, 2, 3)").unwrap();
        match ast {
            AstNode::Function { name, args, is_builtin } => {
                assert_eq!(name, "sum");
                assert_eq!(args.len(), 3);
                assert_eq!(args[0], AstNode::Number(1.0));
                assert_eq!(args[1], AstNode::Number(2.0));
                assert_eq!(args[2], AstNode::Number(3.0));
                assert!(!is_builtin); // Bare function call (no $ prefix)
            }
            _ => panic!("Expected Function node"),
        }
    }

    #[test]
    fn test_parse_conditional() {
        let ast = parse("x > 0 ? 1 : -1").unwrap();
        match ast {
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                assert!(matches!(*condition, AstNode::Binary { .. }));
                assert_eq!(*then_branch, AstNode::Number(1.0));
                // Note: Parser optimization - negative number literals are parsed directly
                // as Number(-1.0) rather than Unary { Negate, Number(1.0) }
                assert_eq!(
                    else_branch,
                    Some(Box::new(AstNode::Number(-1.0)))
                );
            }
            _ => panic!("Expected Conditional node"),
        }
    }

    #[test]
    fn test_parse_comparison() {
        let ast = parse("x < 10").unwrap();
        match ast {
            AstNode::Binary { op, .. } => {
                assert_eq!(op, BinaryOp::LessThan);
            }
            _ => panic!("Expected Binary node"),
        }
    }

    #[test]
    fn test_parse_logical_and() {
        let ast = parse("true and false").unwrap();
        match ast {
            AstNode::Binary { op, lhs, rhs } => {
                assert_eq!(op, BinaryOp::And);
                assert_eq!(*lhs, AstNode::Boolean(true));
                assert_eq!(*rhs, AstNode::Boolean(false));
            }
            _ => panic!("Expected Binary node"),
        }
    }

    #[test]
    fn test_parse_string_concatenation() {
        let ast = parse(r#""hello" & " " & "world""#).unwrap();
        match ast {
            AstNode::Binary { op, .. } => {
                assert_eq!(op, BinaryOp::Concatenate);
            }
            _ => panic!("Expected Binary node"),
        }
    }

    #[test]
    fn test_parse_unary_minus() {
        // Note: The parser optimizes negative number literals by parsing them directly
        // as negative numbers (e.g., -5 → Number(-5.0)) rather than creating a Unary
        // node. This is more efficient and the result is semantically equivalent.
        let ast = parse("-5").unwrap();
        assert_eq!(ast, AstNode::Number(-5.0));
    }

    #[test]
    fn test_parse_block() {
        let ast = parse("(1; 2; 3)").unwrap();
        match ast {
            AstNode::Block(expressions) => {
                assert_eq!(expressions.len(), 3);
                assert_eq!(expressions[0], AstNode::Number(1.0));
                assert_eq!(expressions[1], AstNode::Number(2.0));
                assert_eq!(expressions[2], AstNode::Number(3.0));
            }
            _ => panic!("Expected Block node"),
        }
    }

    #[test]
    fn test_parse_complex_expression() {
        // Test a more complex expression
        let ast = parse("(a + b) * c.d").unwrap();
        assert!(matches!(ast, AstNode::Binary { .. }));
    }

    #[test]
    fn test_parse_dollar_function_call() {
        // Test $uppercase function
        let ast = parse(r#"$uppercase("hello")"#).unwrap();
        match ast {
            AstNode::Function { name, args, is_builtin } => {
                assert_eq!(name, "uppercase");
                assert_eq!(args.len(), 1);
                assert_eq!(args[0], AstNode::String("hello".to_string()));
                assert!(is_builtin); // $ prefix means builtin
            }
            _ => panic!("Expected Function node"),
        }

        // Test $sum function
        let ast = parse("$sum([1, 2, 3])").unwrap();
        match ast {
            AstNode::Function { name, args, is_builtin } => {
                assert_eq!(name, "sum");
                assert_eq!(args.len(), 1);
                assert!(is_builtin); // $ prefix means builtin
            }
            _ => panic!("Expected Function node"),
        }
    }

    #[test]
    fn test_parse_nested_dollar_functions() {
        // Test nested $function calls
        let ast = parse(r#"$length($lowercase("HELLO"))"#).unwrap();
        match ast {
            AstNode::Function { name, args, is_builtin } => {
                assert_eq!(name, "length");
                assert_eq!(args.len(), 1);
                assert!(is_builtin);
                // Check nested function
                match &args[0] {
                    AstNode::Function { name: inner_name, is_builtin: inner_builtin, .. } => {
                        assert_eq!(inner_name, "lowercase");
                        assert!(inner_builtin);
                    }
                    _ => panic!("Expected nested Function node"),
                }
            }
            _ => panic!("Expected Function node"),
        }
    }
}
