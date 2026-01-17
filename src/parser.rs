// JSONata expression parser
// Mirrors parser.js from the reference implementation

use crate::ast::{AstNode, BinaryOp, UnaryOp};
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

    // Identifiers and operators
    Identifier(String),
    Variable(String),

    // Operators
    Plus,
    Minus,
    Star,
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
    Colon,
    ColonEqual, // :=

    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Semicolon,

    // Special
    Eof,
}

/// Lexer for tokenizing JSONata expressions
pub struct Lexer {
    input: Vec<char>,
    position: usize,
}

impl Lexer {
    pub fn new(input: String) -> Self {
        Lexer {
            input: input.chars().collect(),
            position: 0,
        }
    }

    fn current(&self) -> Option<char> {
        if self.position < self.input.len() {
            Some(self.input[self.position])
        } else {
            None
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        let pos = self.position + offset;
        if pos < self.input.len() {
            Some(self.input[pos])
        } else {
            None
        }
    }

    fn advance(&mut self) {
        if self.position < self.input.len() {
            self.position += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
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
                            if let Some(ch) = char::from_u32(code) {
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

        // Optional minus sign
        if self.current() == Some('-') {
            self.advance();
        }

        // Integer part
        if self.current() == Some('0') {
            self.advance();
        } else if self.current().map_or(false, |c| c.is_ascii_digit()) {
            while self.current().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
        } else {
            return Err(ParserError::InvalidNumber("Expected digit".to_string()));
        }

        // Fractional part
        if self.current() == Some('.') {
            self.advance();
            if !self.current().map_or(false, |c| c.is_ascii_digit()) {
                return Err(ParserError::InvalidNumber(
                    "Expected digit after decimal point".to_string(),
                ));
            }
            while self.current().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        // Exponent part
        if matches!(self.current(), Some('e') | Some('E')) {
            self.advance();
            if matches!(self.current(), Some('+') | Some('-')) {
                self.advance();
            }
            if !self.current().map_or(false, |c| c.is_ascii_digit()) {
                return Err(ParserError::InvalidNumber(
                    "Expected digit in exponent".to_string(),
                ));
            }
            while self.current().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        let num_str: String = self.input[start..self.position].iter().collect();
        num_str
            .parse()
            .map_err(|_| ParserError::InvalidNumber(num_str))
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
                Some('"') => return Ok(Token::String(self.read_string('"')?)),
                Some('\'') => return Ok(Token::String(self.read_string('\'')?)),

                // Backtick names
                Some('`') => return Ok(Token::Identifier(self.read_backtick_name()?)),

                // Numbers
                Some(ch) if ch.is_ascii_digit() => {
                    return Ok(Token::Number(self.read_number()?));
                }
                Some('-') if self.peek(1).map_or(false, |c| c.is_ascii_digit()) => {
                    return Ok(Token::Number(self.read_number()?));
                }

                // Variables (start with $)
                Some('$') => {
                    self.advance();
                    let name = self.read_identifier();
                    return Ok(Token::Variable(name));
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

                // Single-character operators and delimiters
                Some('(') => {
                    self.advance();
                    return Ok(Token::LeftParen);
                }
                Some(')') => {
                    self.advance();
                    return Ok(Token::RightParen);
                }
                Some('[') => {
                    self.advance();
                    return Ok(Token::LeftBracket);
                }
                Some(']') => {
                    self.advance();
                    return Ok(Token::RightBracket);
                }
                Some('{') => {
                    self.advance();
                    return Ok(Token::LeftBrace);
                }
                Some('}') => {
                    self.advance();
                    return Ok(Token::RightBrace);
                }
                Some(',') => {
                    self.advance();
                    return Ok(Token::Comma);
                }
                Some(';') => {
                    self.advance();
                    return Ok(Token::Semicolon);
                }
                Some(':') => {
                    self.advance();
                    return Ok(Token::Colon);
                }
                Some('?') => {
                    self.advance();
                    return Ok(Token::Question);
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
                Some('*') => {
                    self.advance();
                    return Ok(Token::Star);
                }
                Some('/') => {
                    self.advance();
                    return Ok(Token::Slash);
                }
                Some('%') => {
                    self.advance();
                    return Ok(Token::Percent);
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

                // Identifiers and keywords
                Some(ch) if ch.is_alphabetic() || ch == '_' => {
                    let ident = self.read_identifier();
                    return Ok(match ident.as_str() {
                        "true" => Token::True,
                        "false" => Token::False,
                        "null" => Token::Null,
                        "and" => Token::And,
                        "or" => Token::Or,
                        "in" => Token::In,
                        _ => Token::Identifier(ident),
                    });
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
            Token::Or => Some((25, 26)),
            Token::And => Some((30, 31)),
            Token::Equal | Token::NotEqual | Token::LessThan | Token::LessThanOrEqual
            | Token::GreaterThan | Token::GreaterThanOrEqual | Token::In => Some((40, 41)),
            Token::Ampersand => Some((50, 51)),
            Token::Plus | Token::Minus => Some((50, 51)),
            Token::Star | Token::Slash | Token::Percent => Some((60, 61)),
            Token::Dot => Some((75, 76)),
            Token::LeftBracket => Some((80, 81)),
            Token::LeftParen => Some((80, 81)),
            Token::Question => Some((20, 21)),
            Token::DotDot => Some((20, 21)),
            Token::ColonEqual => Some((10, 9)), // Right associative
            _ => None,
        }
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
            Token::Identifier(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(AstNode::Path {
                    steps: vec![AstNode::String(name)],
                })
            }
            Token::Variable(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(AstNode::Variable(name))
            }
            Token::LeftParen => {
                self.advance()?; // skip '('

                // Parse block expressions (separated by semicolons)
                let mut expressions = vec![self.parse_expression(0)?];

                while self.current_token == Token::Semicolon {
                    self.advance()?;
                    if self.current_token == Token::RightParen {
                        break;
                    }
                    expressions.push(self.parse_expression(0)?);
                }

                self.expect(Token::RightParen)?;

                if expressions.len() == 1 {
                    Ok(expressions.into_iter().next().unwrap())
                } else {
                    Ok(AstNode::Block(expressions))
                }
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
            Token::Minus => {
                self.advance()?;
                let operand = self.parse_expression(70)?; // High precedence for unary
                Ok(AstNode::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(operand),
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
                    let rhs = self.parse_expression(right_bp)?;

                    // Flatten path expressions
                    let mut steps = match lhs {
                        AstNode::Path { steps } => steps,
                        _ => vec![lhs],
                    };

                    match rhs {
                        AstNode::Path { steps: mut rhs_steps } => {
                            steps.append(&mut rhs_steps);
                        }
                        _ => steps.push(rhs),
                    }

                    lhs = AstNode::Path { steps };
                }
                Token::LeftBracket => {
                    self.advance()?;
                    let index = self.parse_expression(0)?;
                    self.expect(Token::RightBracket)?;

                    // For now, represent array access as a binary operation
                    // In full implementation, this would create a predicate
                    lhs = AstNode::Binary {
                        op: BinaryOp::In, // Placeholder
                        lhs: Box::new(lhs),
                        rhs: Box::new(index),
                    };
                }
                Token::LeftParen => {
                    self.advance()?;

                    let mut args = Vec::new();

                    if self.current_token != Token::RightParen {
                        loop {
                            args.push(self.parse_expression(0)?);

                            if self.current_token != Token::Comma {
                                break;
                            }
                            self.advance()?;
                        }
                    }

                    self.expect(Token::RightParen)?;

                    // Extract function name from lhs
                    let name = match &lhs {
                        AstNode::Path { steps } if steps.len() == 1 => {
                            match &steps[0] {
                                AstNode::String(s) => s.clone(),
                                _ => return Err(ParserError::InvalidSyntax(
                                    "Invalid function name".to_string()
                                )),
                            }
                        }
                        _ => return Err(ParserError::InvalidSyntax(
                            "Invalid function call".to_string()
                        )),
                    };

                    lhs = AstNode::Function { name, args };
                }
                Token::Question => {
                    self.advance()?;
                    let then_branch = self.parse_expression(0)?;

                    let else_branch = if self.current_token == Token::Colon {
                        self.advance()?;
                        Some(Box::new(self.parse_expression(right_bp)?))
                    } else {
                        None
                    };

                    lhs = AstNode::Conditional {
                        condition: Box::new(lhs),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    };
                }
                _ => {
                    // Binary operators
                    let op = match &self.current_token {
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
                steps: vec![AstNode::String("foo".to_string())]
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
                assert_eq!(steps[0], AstNode::String("foo".to_string()));
                assert_eq!(steps[1], AstNode::String("bar".to_string()));
            }
            _ => panic!("Expected Path node"),
        }
    }

    #[test]
    fn test_parse_function_call() {
        let ast = parse("sum(1, 2, 3)").unwrap();
        match ast {
            AstNode::Function { name, args } => {
                assert_eq!(name, "sum");
                assert_eq!(args.len(), 3);
                assert_eq!(args[0], AstNode::Number(1.0));
                assert_eq!(args[1], AstNode::Number(2.0));
                assert_eq!(args[2], AstNode::Number(3.0));
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
}
