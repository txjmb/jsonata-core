// JSONata expression parser
// Mirrors parser.js from the reference implementation

#![allow(clippy::approx_constant)]

use crate::ast::{AstNode, BinaryOp, PathStep, Stage, UnaryOp};
use thiserror::Error;

/// Parser errors
/// The JSONata code for a token that turned up where an operand was expected.
///
/// jsonata-js splits this two ways: a *recognised* symbol used in prefix
/// position is S0211 ("cannot be used as a unary operator"), while something
/// that is not an operator at all is S0204 ("Unknown operator"). End of input
/// is neither -- it is S0207.
fn unexpected_token_message(token: &str) -> String {
    if token == "Eof" {
        return "S0207: Unexpected end of expression".to_string();
    }
    // A named token variant (`RightParen`, `GreaterThan`, `At`, `Colon`, ...)
    // is a symbol the lexer knows; a bare punctuation character is not.
    if token.chars().next().is_some_and(char::is_alphabetic) {
        format!("S0211: The symbol {token} cannot be used as a unary operator")
    } else {
        format!("S0204: Unknown operator: {token}")
    }
}

/// `\u` needs four hex digits (S0104); any other escape is simply unsupported
/// (S0103).
fn invalid_escape_message(seq: &str) -> String {
    if seq.starts_with('u') || seq.starts_with("\\u") {
        "S0104: The escape sequence \\u must be followed by 4 hex digits".to_string()
    } else {
        format!(
            "S0103: Unsupported escape sequence: \\{}",
            seq.trim_start_matches('\\')
        )
    }
}

/// Running out of input while expecting something is S0203; finding the wrong
/// thing is S0202. A missing *parameter name* is its own code, S0208.
fn expected_message(expected: &str, found: &str) -> String {
    if expected == "parameter name" {
        return "S0208: Parameter of function definition must be a variable name (start with $)"
            .to_string();
    }
    if found == "Eof" {
        format!("S0203: Expected {expected} before end of expression")
    } else {
        format!("S0202: Expected {expected}, got {found}")
    }
}

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("{}", unexpected_token_message(.0))]
    UnexpectedToken(String),

    #[error("S0207: Unexpected end of expression")]
    UnexpectedEnd,

    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),

    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    #[error("S0101: String literal must be terminated by a matching quote")]
    UnclosedString,

    #[error("{}", invalid_escape_message(.0))]
    InvalidEscape(String),

    #[error("Unclosed comment")]
    UnclosedComment,

    #[error("S0105: Quoted property name must be terminated with a backquote (`)")]
    UnclosedBacktick,

    #[error("{}", expected_message(expected, found))]
    Expected { expected: String, found: String },

    /// A JSONata-spec-coded parse error (S0214-S0217 for the %/@ operators).
    /// Code is at the start of the message (matching the DateTimeError::Coded
    /// convention from the datetime picture-string engine) so
    /// test_reference_suite.py's extract_error_code() finds it.
    #[error("{code}: {message}")]
    Coded { code: &'static str, message: String },
}

impl ParserError {
    /// The full display-ready message: `Coded` variants (e.g. S0214) are
    /// already exactly "code: message" via `Display`, so they pass
    /// through unchanged; every other variant gets a "Parse error: "
    /// prefix added. Used by both the Python bindings (`src/lib.rs`) and
    /// the `jsonata` CLI.
    pub fn display_message(&self) -> String {
        let msg = self.to_string();
        if matches!(self, ParserError::Coded { .. }) {
            msg
        } else {
            format!("Parse error: {}", msg)
        }
    }
}

#[cfg(test)]
mod parser_error_display_message_tests {
    use super::ParserError;

    #[test]
    fn coded_error_passes_through_unchanged() {
        let e = ParserError::Coded {
            code: "S0214",
            message: "The % operator is invalid outside a path".to_string(),
        };
        assert_eq!(
            e.display_message(),
            "S0214: The % operator is invalid outside a path"
        );
    }

    #[test]
    fn uncoded_error_gets_parse_error_prefix() {
        // `InvalidNumber` is one of the variants that still carries no JSONata
        // code. It used to be `UnexpectedToken`, which now always produces one
        // (S0204/S0207/S0211) and so no longer exercises the uncoded path.
        let e = ParserError::InvalidNumber("1e999".to_string());
        assert_eq!(e.display_message(), "Parse error: Invalid number: 1e999");
    }

    /// The variants that gained codes render them *inside* the message, so the
    /// `Parse error:` prefix still applies -- `display_message` only strips the
    /// prefix for `Coded`, which carries its code structurally.
    #[test]
    fn variant_derived_codes_appear_in_the_message() {
        for (error, expected) in [
            (
                ParserError::UnexpectedToken("Eof".to_string()),
                "Parse error: S0207: Unexpected end of expression",
            ),
            (
                ParserError::UnexpectedToken("At".to_string()),
                "Parse error: S0211: The symbol At cannot be used as a unary operator",
            ),
            (
                ParserError::UnexpectedToken("!".to_string()),
                "Parse error: S0204: Unknown operator: !",
            ),
            (
                ParserError::UnclosedString,
                "Parse error: S0101: String literal must be terminated by a matching quote",
            ),
            (
                ParserError::Expected {
                    expected: "RightParen".to_string(),
                    found: "Eof".to_string(),
                },
                "Parse error: S0203: Expected RightParen before end of expression",
            ),
            (
                ParserError::Expected {
                    expected: "RightBracket".to_string(),
                    found: "Colon".to_string(),
                },
                "Parse error: S0202: Expected RightBracket, got Colon",
            ),
        ] {
            assert_eq!(error.display_message(), expected);
        }
    }
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
    Undefined,                                // The `undefined` keyword
    Regex { pattern: String, flags: String }, // /pattern/flags

    // Identifiers and operators
    Identifier(String),
    Variable(String),
    ParentVariable(String), // $$ variables
    Function,               // function keyword

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
    QuestionColon,    // ?:
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
    Hash, // # index binding operator
    At,   // @ focus binding operator
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
                                            let cp =
                                                0x10000 + (code - 0xD800) * 0x400 + (low - 0xDC00);
                                            if let Some(ch) = char::from_u32(cp) {
                                                result.push(ch);
                                            } else {
                                                return Err(ParserError::InvalidEscape(format!(
                                                    "\\u{}\\u{}",
                                                    hex, low_hex
                                                )));
                                            }
                                        } else {
                                            return Err(ParserError::InvalidEscape(format!(
                                                "\\u{}\\u{}",
                                                hex, low_hex
                                            )));
                                        }
                                    } else {
                                        return Err(ParserError::InvalidEscape(format!(
                                            "\\u{}",
                                            hex
                                        )));
                                    }
                                } else {
                                    return Err(ParserError::InvalidEscape(format!("\\u{}", hex)));
                                }
                            } else if let Some(ch) = char::from_u32(code) {
                                result.push(ch);
                            } else {
                                return Err(ParserError::InvalidEscape(format!("\\u{}", hex)));
                            }
                            continue; // Don't advance again
                        }
                        Some(ch) => return Err(ParserError::InvalidEscape(format!("\\{}", ch))),
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
            return Err(ParserError::InvalidNumber(format!(
                "S0102: Number out of range: {}",
                num_str
            )));
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
                        Some(Token::LeftParen)
                        | Some(Token::LeftBracket)
                        | Some(Token::LeftBrace) => true,
                        Some(Token::Comma) | Some(Token::Semicolon) | Some(Token::Colon) => true,
                        Some(Token::Equal) | Some(Token::NotEqual) => true,
                        Some(Token::LessThan) | Some(Token::LessThanOrEqual) => true,
                        Some(Token::GreaterThan) | Some(Token::GreaterThanOrEqual) => true,
                        Some(Token::Plus) | Some(Token::Minus) | Some(Token::Star)
                        | Some(Token::Percent) => true,
                        Some(Token::Ampersand)
                        | Some(Token::Question)
                        | Some(Token::TildeArrow) => true,
                        Some(Token::ColonEqual)
                        | Some(Token::QuestionQuestion)
                        | Some(Token::QuestionColon) => true,
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
                Some('@') => {
                    self.advance();
                    return Ok(Token::At);
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

// Same constants `evaluate_internal` (src/evaluator.rs) and
// ast_transform.rs's `resolve_ancestry` use for their analogous
// native-stack safety nets -- see those for the full rationale. Kept as
// separate constants (rather than reused from those modules) since this
// module has no dependency on them and the guards are conceptually
// independent safety nets.
const PARSER_RED_ZONE: usize = 128 * 1024;
const PARSER_GROW_STACK_SIZE: usize = 8 * 1024 * 1024;

// Same ceiling as ast_transform.rs's MAX_TRANSFORM_DEPTH (1000), chosen for
// consistency: a tree that passes THIS guard (enforced earlier, at parse
// construction time) should also comfortably pass ast_transform's guard
// afterward, which stays in place as harmless defense-in-depth. Empirically
// verified (this task, scratch test, 1MB-stack thread, release build) that
// n=999 succeeds and n=1001 fails gracefully with U1002 for BOTH the
// deeply-parenthesized case (real recursive descent per level, the more
// expensive of the two shapes since `stacker::maybe_grow`'s growth still
// costs real stack per `parse_expression` call) and the flat-arithmetic
// loop-driven case -- so 1000 has essentially zero headroom by construction
// (that's the point: it's an exact ceiling match with ast_transform, not an
// independently-tuned one) but is not itself unsafe at that ceiling.
const MAX_PARSE_DEPTH: usize = 1000;

/// Parser for JSONata expressions using Pratt parsing
pub struct Parser {
    lexer: Lexer,
    current_token: Token,
    /// Current expression-nesting depth, shared by two structurally
    /// different growth patterns (see `parse_expression`/
    /// `parse_expression_impl`): recursive descent (parens, unary
    /// operands, array/object elements, function args, blocks -- depth
    /// grows by 1 per actual recursive call to `parse_expression` and
    /// shrinks back on return) and loop-driven left-nesting (flat
    /// `1+1+1+...` chains -- a SINGLE call's `loop { .. }` reassigns `lhs`
    /// to a deeper node every iteration without a new recursive call).
    /// Both must be bounded by this one counter; see `MAX_PARSE_DEPTH`.
    depth: usize,
    /// An S0213 ("literal value cannot be used as a step") noticed while
    /// parsing, held until the whole expression has parsed.
    ///
    /// jsonata-js raises S0213 from `processAST`, a pass that runs after
    /// parsing, so a *parse* error beats it: `$.7a` is S0201 for the
    /// unexpected trailing `a`, even though the `7` step is also invalid.
    /// Raising it inline made us answer S0213 there. Deferring restores the
    /// ordering without moving the check itself into a post-parse pass.
    pending_literal_step: Option<String>,
}

impl Parser {
    pub fn new(input: String) -> Result<Self, ParserError> {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token()?;
        Ok(Parser {
            lexer,
            current_token,
            depth: 0,
            pending_literal_step: None,
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
            Token::Equal
            | Token::NotEqual
            | Token::LessThan
            | Token::LessThanOrEqual
            | Token::GreaterThan
            | Token::GreaterThanOrEqual
            | Token::In => Some((40, 41)),
            Token::Ampersand => Some((50, 51)),
            Token::Plus | Token::Minus => Some((50, 51)),
            Token::Star | Token::Slash | Token::Percent => Some((60, 61)),
            Token::Dot => Some((75, 85)), // Right bp is higher to prevent consuming postfix operators
            Token::LeftBracket => Some((80, 81)),
            Token::LeftParen => Some((80, 81)),
            Token::LeftBrace => Some((80, 81)), // Object constructor as postfix
            Token::Caret => Some((80, 81)),     // Sort operator as postfix
            Token::Hash => Some((80, 81)),      // Index binding operator as postfix
            Token::At => Some((80, 81)),        // Focus binding operator as postfix
            Token::Question => Some((20, 21)),
            Token::QuestionQuestion => Some((15, 16)), // Coalescing operator
            Token::QuestionColon => Some((15, 16)),    // Default operator
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
                Token::Plus => {
                    signature.push('+');
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
                    let c = if self.current_token == Token::LeftParen {
                        '('
                    } else {
                        ')'
                    };
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
                Ok(AstNode::Regex {
                    pattern: pat,
                    flags: flg,
                })
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
            Token::Percent => {
                // Parent operator in primary position. Label is resolved by
                // ast_transform -- this empty string is never observed by
                // the evaluator (ast_transform fills every AstNode::Parent
                // ("") with a real label or errors S0217).
                self.advance()?;
                Ok(AstNode::Parent(String::new()))
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

    /// Bumps the shared nesting-depth counter and errors out gracefully
    /// (`U1002`) once `MAX_PARSE_DEPTH` is exceeded, instead of letting
    /// either growth pattern (recursive descent or loop-driven
    /// left-nesting -- see `Parser::depth`'s doc comment) overflow the
    /// native stack. New error code `U1002` (no jsonata-js equivalent):
    /// reuses the same code `ast_transform.rs`'s `resolve_ancestry` guard
    /// introduced for the same conceptual guard, enforced earlier in the
    /// pipeline (at raw-parse construction time, before ast_transform ever
    /// runs).
    fn bump_parse_depth(&mut self) -> Result<(), ParserError> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return Err(ParserError::Coded {
                code: "U1002",
                message: format!(
                    "Stack overflow - maximum expression nesting depth ({}) exceeded while parsing expression",
                    MAX_PARSE_DEPTH
                ),
            });
        }
        Ok(())
    }

    /// Parse an expression with Pratt parsing.
    ///
    /// Thin wrapper around `parse_expression_impl` that owns the
    /// entry-depth push/pop (matching `evaluate_internal`'s
    /// `stacker::maybe_grow` pattern in src/evaluator.rs): every actual
    /// recursive call to `parse_expression` (parens, unary operands,
    /// array/object elements, function args, blocks, ...) bumps `self.depth`
    /// on entry and restores it to the pre-call value on return, so depth
    /// accumulated while parsing one subtree never leaks into an unrelated
    /// sibling subtree parsed afterward. Loop-driven left-nesting (the flat
    /// `1+1+1+...` case, where `parse_expression_impl`'s own `loop { .. }`
    /// reassigns `lhs` to a deeper node every iteration WITHOUT a new
    /// recursive call) is bounded separately, by additional `bump_parse_depth`
    /// calls at each `lhs = ..` reassignment site inside that loop.
    fn parse_expression(&mut self, min_bp: u8) -> Result<AstNode, ParserError> {
        let entry_depth = self.depth;
        self.bump_parse_depth()?;

        let result = stacker::maybe_grow(PARSER_RED_ZONE, PARSER_GROW_STACK_SIZE, || {
            self.parse_expression_impl(min_bp)
        });

        self.depth = entry_depth;
        result
    }

    fn parse_expression_impl(&mut self, min_bp: u8) -> Result<AstNode, ParserError> {
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
                        self.bump_parse_depth()?;
                        lhs = AstNode::Path { steps };
                    } else if self.current_token == Token::LeftParen {
                        // Check for .(expr) syntax (function application)
                        self.advance()?;

                        // Empty block `.()`: a parenthesised step with no
                        // expression (e.g. `Account.Order.().%`). jsonata-js
                        // treats `()` as an empty block that evaluates to
                        // undefined; keep it as a `FunctionApplication` of an
                        // empty `Block` so the ancestry pass can walk past it.
                        if self.current_token == Token::RightParen {
                            self.advance()?;
                            let mut steps = match lhs {
                                AstNode::Path { steps } => steps,
                                _ => vec![PathStep::new(lhs)],
                            };
                            steps.push(PathStep::new(AstNode::FunctionApplication(Box::new(
                                AstNode::Block(Vec::new()),
                            ))));
                            self.bump_parse_depth()?;
                            lhs = AstNode::Path { steps };
                            continue;
                        }

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
                        self.bump_parse_depth()?;
                        lhs = AstNode::Path { steps };
                    } else {
                        // Normal dot path
                        let rhs = self.parse_expression(right_bp)?;

                        // Flatten path expressions
                        let mut steps = match lhs {
                            AstNode::Path { steps } => steps,
                            // Convert string literals to field names when used as first step in path
                            // e.g., "foo".bar should behave like foo.bar
                            AstNode::String(field_name) => {
                                vec![PathStep::new(AstNode::Name(field_name))]
                            }
                            _ => vec![PathStep::new(lhs)],
                        };

                        // S0213: The literal value cannot be used as a step within a path expression
                        // Numbers, booleans (true/false), and null cannot be path steps
                        let literal_step = match &rhs {
                            AstNode::Number(n) => Some(n.to_string()),
                            AstNode::Boolean(b) => Some(b.to_string()),
                            AstNode::Null => Some("null".to_string()),
                            _ => None,
                        };
                        if let Some(literal) = literal_step {
                            // Recorded, not raised: see `pending_literal_step`.
                            // The first one wins, matching a post-parse pass
                            // walking the tree in order.
                            if self.pending_literal_step.is_none() {
                                self.pending_literal_step = Some(format!(
                                    "S0213: The literal value {} cannot be used as a step within a path expression",
                                    literal
                                ));
                            }
                        }

                        match rhs {
                            AstNode::Path {
                                steps: mut rhs_steps,
                            } => {
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

                            // `[]` is the keepSingleton marker, not the filter
                            // `[true]`: it keeps the result an array instead of
                            // filtering and unwrapping. They parsed to the same
                            // node until KeepArray split them.
                            let stage = if self.current_token == Token::RightBracket {
                                Stage::KeepArray
                            } else {
                                Stage::Filter(Box::new(self.parse_expression(0)?))
                            };

                            self.expect(Token::RightBracket)?;

                            // Attach predicate as stage to the last step
                            if let Some(last_step) = steps.last_mut() {
                                last_step.stages.push(stage);
                            }
                        }

                        self.bump_parse_depth()?;
                        lhs = AstNode::Path { steps };
                    }
                }
                Token::LeftBracket => {
                    // S0209: A predicate cannot follow a grouping expression in a step
                    // Check if lhs is an ObjectTransform (grouping expression)
                    if matches!(lhs, AstNode::ObjectTransform { .. }) {
                        return Err(ParserError::InvalidSyntax(
                            "S0209: A predicate cannot follow a grouping expression in a step"
                                .to_string(),
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

                        steps.push(PathStep::new(AstNode::KeepArray));
                        self.bump_parse_depth()?;
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
                        self.bump_parse_depth()?;
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
                            self.bump_parse_depth()?;
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
                                        _ => {
                                            return Err(ParserError::InvalidSyntax(
                                                "Invalid function name".to_string(),
                                            ))
                                        }
                                    };
                                    self.bump_parse_depth()?;
                                    lhs = AstNode::Function {
                                        name,
                                        args,
                                        is_builtin: false,
                                    };
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
                                        context_steps.push(PathStep::new(
                                            AstNode::FunctionApplication(Box::new(func_call)),
                                        ));

                                        self.bump_parse_depth()?;
                                        lhs = AstNode::Path {
                                            steps: context_steps,
                                        };
                                    }
                                    // Check if last step is a Lambda (inline function in path)
                                    else if let AstNode::Lambda {
                                        params,
                                        body,
                                        signature,
                                        thunk,
                                    } = last_step
                                    {
                                        // Extract all but the last step as the path context
                                        let mut context_steps = steps.clone();
                                        context_steps.pop();

                                        // In path context, determine if we need to prepend $
                                        // - If fewer args than params, prepend $ (context value) as first arg
                                        // - If args == params, use args as-is
                                        let full_args = if args.len() < params.len() {
                                            let mut new_args =
                                                vec![AstNode::Variable("$".to_string())];
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
                                                lhs: Box::new(AstNode::Variable(
                                                    "__path_lambda__".to_string(),
                                                )),
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
                                        context_steps.push(PathStep::new(
                                            AstNode::FunctionApplication(Box::new(
                                                lambda_invocation,
                                            )),
                                        ));

                                        self.bump_parse_depth()?;
                                        lhs = AstNode::Path {
                                            steps: context_steps,
                                        };
                                    } else {
                                        return Err(ParserError::InvalidSyntax(
                                            "Invalid function call".to_string(),
                                        ));
                                    }
                                }
                                // Handle $-prefixed function names: $uppercase()
                                AstNode::Variable(name) => {
                                    self.bump_parse_depth()?;
                                    lhs = AstNode::Function {
                                        name: name.clone(),
                                        args,
                                        is_builtin: true,
                                    };
                                }
                                _ => {
                                    return Err(ParserError::InvalidSyntax(
                                        "Invalid function call".to_string(),
                                    ))
                                }
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

                    self.bump_parse_depth()?;
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
                    self.bump_parse_depth()?;
                    lhs = AstNode::ObjectTransform {
                        input: Box::new(lhs),
                        pattern: pairs,
                    };
                }
                Token::Hash => {
                    // Index binding operator: #$var
                    // Binds the current array index to the specified variable
                    self.advance()?; // skip '#'

                    // Expect a variable name. A bare `#` without a `$var` (e.g.
                    // `Account.Order@$o#i.Product`) is S0214, mirroring jsonata-js's
                    // inline check for `@`/`#` (parser.js ~L834-847).
                    let var_name = match &self.current_token {
                        Token::Variable(name) => name.clone(),
                        _ => {
                            return Err(ParserError::Coded {
                                code: "S0214",
                                message: "Expected a variable reference after #".to_string(),
                            });
                        }
                    };
                    self.advance()?; // skip variable

                    // Produces a generic Binary(IndexBind) marker -- ast_transform
                    // resolves this into a PathStep.index_var flag, mirroring how
                    // @$var/FocusBind is represented (see Token::At below). Using
                    // the same generic Binary shape (rather than a dedicated
                    // AstNode::IndexBind variant) lets that variant be retired
                    // from ast.rs entirely.
                    self.bump_parse_depth()?;
                    lhs = AstNode::Binary {
                        op: BinaryOp::IndexBind,
                        lhs: Box::new(lhs),
                        rhs: Box::new(AstNode::Variable(var_name)),
                    };
                }
                Token::At => {
                    // Focus binding operator: @$var
                    // Produces a generic Binary(FocusBind) marker -- ast_transform
                    // resolves this into a PathStep.focus flag, matching
                    // jsonata-js's parser.js:834-847 (which does the same S0214
                    // check inline, deferring all other semantics to processAST).
                    self.advance()?; // skip '@'

                    let var_name = match &self.current_token {
                        Token::Variable(name) => name.clone(),
                        _ => {
                            return Err(ParserError::Coded {
                                code: "S0214",
                                message: "Expected a variable reference after @".to_string(),
                            });
                        }
                    };
                    self.advance()?; // skip variable

                    self.bump_parse_depth()?;
                    lhs = AstNode::Binary {
                        op: BinaryOp::FocusBind,
                        lhs: Box::new(lhs),
                        rhs: Box::new(AstNode::Variable(var_name)),
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

                    self.bump_parse_depth()?;
                    lhs = AstNode::Sort {
                        input: Box::new(lhs),
                        terms,
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

                    self.bump_parse_depth()?;
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

        // A token left over is S0201 in jsonata-js -- its general "syntax
        // error at this token" -- and it beats a deferred S0213, which the
        // reference only reaches in its post-parse pass.
        if self.current_token != Token::Eof {
            return Err(ParserError::InvalidSyntax(format!(
                "S0201: Syntax error: {:?}",
                self.current_token
            )));
        }

        if let Some(message) = self.pending_literal_step.take() {
            return Err(ParserError::InvalidSyntax(message));
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
            AstNode::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                let then_is_tail = Self::is_tail_call(then_branch);
                let else_is_tail = else_branch.as_ref().is_some_and(|e| Self::is_tail_call(e));
                // At least one branch should be a tail call for TCO to be useful
                then_is_tail || else_is_tail
            }

            // Block: last expression is tail position
            AstNode::Block(exprs) => exprs.last().is_some_and(Self::is_tail_call),

            // Variable binding with result: the result expression is tail position
            AstNode::Binary {
                op: BinaryOp::ColonEqual,
                rhs,
                ..
            } => {
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
/// This is the main entry point for parsing. Runs the post-parse
/// ast_transform pass (ancestor-slot resolution, @/#/% unification)
/// unconditionally, matching jsonata-js's processAST always running
/// immediately after the raw Pratt parse.
pub fn parse(expression: &str) -> Result<AstNode, ParserError> {
    let mut parser = Parser::new(expression.to_string())?;
    let raw_ast = parser.parse()?;
    crate::ast_transform::resolve_ancestry(raw_ast).map_err(|e| match e {
        crate::ast_transform::AstTransformError::Coded { code, message } => {
            ParserError::Coded { code, message }
        }
    })
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
        // Negation is handled as a unary operator, not part of the number literal
        assert_eq!(lexer.next_token().unwrap(), Token::Minus);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(10.0));
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
        // "and", "or", "in" are now contextual keywords - lexer emits them as identifiers
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("and".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("or".to_string())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Identifier("in".to_string())
        );
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
        // Test non-slash operators first
        let mut lexer = Lexer::new("+ - * % = != < <= > >= & . .. := ".to_string());

        assert_eq!(lexer.next_token().unwrap(), Token::Plus);
        assert_eq!(lexer.next_token().unwrap(), Token::Minus);
        assert_eq!(lexer.next_token().unwrap(), Token::Star);
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

        // Slash is context-dependent: after a value token it's division, otherwise regex.
        // Test slash after a number (value context) to get Token::Slash.
        let mut lexer2 = Lexer::new("42 / 2".to_string());
        assert_eq!(lexer2.next_token().unwrap(), Token::Number(42.0));
        assert_eq!(lexer2.next_token().unwrap(), Token::Slash);
        assert_eq!(lexer2.next_token().unwrap(), Token::Number(2.0));
        assert_eq!(lexer2.next_token().unwrap(), Token::Eof);
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

        // Path with two steps: Name("foo") and the keepSingleton marker.
        if let AstNode::Path { steps } = ast {
            assert_eq!(steps.len(), 2);
            assert!(matches!(steps[0].node, AstNode::Name(ref s) if s == "foo"));
            assert!(matches!(steps[1].node, AstNode::KeepArray));
        } else {
            panic!("Expected Path, got {:?}", ast);
        }
    }

    /// `[]` and `[true]` are different operators and must not share a node.
    ///
    /// They did until `AstNode::KeepArray` split them, which made one of the
    /// two wrong for every input: `foo[]` keeps the array while `foo[true]`
    /// filters and then unwraps a lone result.
    #[test]
    fn test_empty_brackets_differ_from_literal_true_predicate() {
        let mut parser = Parser::new("foo[true]".to_string()).unwrap();
        let ast = parser.parse().unwrap();

        if let AstNode::Path { steps } = ast {
            assert_eq!(steps.len(), 2);
            match &steps[1].node {
                AstNode::Predicate(pred) => {
                    assert!(matches!(**pred, AstNode::Boolean(true)))
                }
                other => panic!("Expected Predicate, got {:?}", other),
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
        // (1 + 2) * 3 should parse as Block([1 + 2]) * 3
        // Parenthesized expressions always create a Block in JSONata
        let ast = parse("(1 + 2) * 3").unwrap();
        match ast {
            AstNode::Binary {
                op: BinaryOp::Multiply,
                lhs,
                rhs,
            } => {
                match *lhs {
                    AstNode::Block(ref exprs) => {
                        assert_eq!(exprs.len(), 1);
                        match &exprs[0] {
                            AstNode::Binary {
                                op: BinaryOp::Add,
                                lhs,
                                rhs,
                            } => {
                                assert_eq!(**lhs, AstNode::Number(1.0));
                                assert_eq!(**rhs, AstNode::Number(2.0));
                            }
                            _ => panic!("Expected Binary node for addition inside block"),
                        }
                    }
                    _ => panic!(
                        "Expected Block node for parenthesized expression, got {:?}",
                        lhs
                    ),
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
            AstNode::Function {
                name,
                args,
                is_builtin,
            } => {
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
                // Negative numbers are parsed as Unary { Negate, Number(1.0) }
                assert_eq!(
                    else_branch,
                    Some(Box::new(AstNode::Unary {
                        op: UnaryOp::Negate,
                        operand: Box::new(AstNode::Number(1.0)),
                    }))
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
        // Negative numbers are parsed as Unary { Negate, Number(5.0) }
        let ast = parse("-5").unwrap();
        assert_eq!(
            ast,
            AstNode::Unary {
                op: UnaryOp::Negate,
                operand: Box::new(AstNode::Number(5.0)),
            }
        );
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
            AstNode::Function {
                name,
                args,
                is_builtin,
            } => {
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
            AstNode::Function {
                name,
                args,
                is_builtin,
            } => {
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
            AstNode::Function {
                name,
                args,
                is_builtin,
            } => {
                assert_eq!(name, "length");
                assert_eq!(args.len(), 1);
                assert!(is_builtin);
                // Check nested function
                match &args[0] {
                    AstNode::Function {
                        name: inner_name,
                        is_builtin: inner_builtin,
                        ..
                    } => {
                        assert_eq!(inner_name, "lowercase");
                        assert!(inner_builtin);
                    }
                    _ => panic!("Expected nested Function node"),
                }
            }
            _ => panic!("Expected Function node"),
        }
    }

    #[test]
    fn test_signature_with_repeat_modifier_parses() {
        // A lambda literal with a '+' (one-or-more) signature modifier must parse
        // without error. This was rejected before jsonata-js 2.2.1 compatibility work
        // ("Unexpected token in signature: Plus").
        let result = parse("λ($arg1, $arg2)<n+n:o>{{\"a\": $arg1, \"b\": $arg2}}(1, 2, 3)");
        assert!(
            result.is_ok(),
            "expected parse to succeed, got {:?}",
            result
        );
    }

    #[test]
    fn test_parent_operator_parses_as_prefix() {
        // Tests the RAW grammar rule (bare `%` in primary position) in
        // isolation from ast_transform's ancestor resolution -- uses
        // Parser::new(...).parse() directly rather than the free `parse()`
        // function, since the free function now runs ast_transform
        // unconditionally (Step 8), and `%.OrderID` alone has no preceding
        // step for `%` to resolve against (that's covered by
        // ast_transform's own S0217 tests).
        let mut parser = Parser::new("%.OrderID".to_string()).unwrap();
        let ast = parser.parse().unwrap();
        // %.OrderID should parse as a path with two steps: Parent, then Name("OrderID")
        match ast {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 2);
                assert!(matches!(steps[0].node, AstNode::Parent(_)));
                assert!(matches!(steps[1].node, AstNode::Name(ref n) if n == "OrderID"));
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_percent_still_parses_as_modulo_infix() {
        // Regression: % must still work as binary modulo when NOT in prefix position
        let ast = parse("10 % 3").unwrap();
        assert!(matches!(
            ast,
            AstNode::Binary {
                op: BinaryOp::Modulo,
                ..
            }
        ));
    }

    #[test]
    fn test_focus_bind_parses_as_binary_marker() {
        // Tests the RAW grammar rule in isolation from ast_transform (which
        // now runs unconditionally in the free `parse()` and would rewrite
        // this into a Path with a `focus` flag instead -- see
        // ast_transform.rs's own real-parser-based tests for that).
        let mut parser = Parser::new("Order@$o".to_string()).unwrap();
        let ast = parser.parse().unwrap();
        match ast {
            AstNode::Binary {
                op: BinaryOp::FocusBind,
                lhs,
                rhs,
            } => {
                // Order is parsed as a Path with a Name step
                if let AstNode::Path { steps } = *lhs {
                    assert_eq!(steps.len(), 1);
                    assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "Order"));
                } else {
                    panic!("expected lhs to be Path, got {:?}", lhs);
                }
                assert!(matches!(*rhs, AstNode::Variable(ref n) if n == "o"));
            }
            other => panic!("expected Binary{{FocusBind}}, got {:?}", other),
        }
    }

    #[test]
    fn test_index_bind_parses_as_binary_marker() {
        // #$var now reuses the same generic Binary marker shape as @$var
        // (Task 4 retires the dedicated AstNode::IndexBind struct variant).
        let mut parser = Parser::new("arr#$i".to_string()).unwrap();
        let ast = parser.parse().unwrap();
        match ast {
            AstNode::Binary {
                op: BinaryOp::IndexBind,
                lhs,
                rhs,
            } => {
                if let AstNode::Path { steps } = *lhs {
                    assert_eq!(steps.len(), 1);
                    assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "arr"));
                } else {
                    panic!("expected lhs to be Path, got {:?}", lhs);
                }
                assert!(matches!(*rhs, AstNode::Variable(ref n) if n == "i"));
            }
            other => panic!("expected Binary{{IndexBind}}, got {:?}", other),
        }
    }

    #[test]
    fn test_focus_bind_requires_variable_rhs() {
        // S0214: @'s RHS must be a bare variable reference
        let err = parse("Order@foo").unwrap_err();
        assert!(err.to_string().starts_with("S0214"));
    }
}
