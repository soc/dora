use std::fmt;
use std::collections::HashMap;

use lexer::reader::{CodeReader,StrReader,FileReader};
use lexer::token::{Token,TokenType};
use lexer::position::Position;
use error::{ParseError,ErrorCode};

pub mod reader;
pub mod token;
pub mod position;

struct CharPos {
    value: char,
    position: Position
}

impl fmt::Show for CharPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "char {} at line {:?}", self.value, self.position)
    }
}

impl Copy for CharPos {}

pub struct Lexer<T : CodeReader> {
    reader: T,
    position: Position,
    eof_reached: bool,
    tabwidth: u32,

    buffer: Vec<CharPos>,
    keywords: HashMap<&'static str, TokenType>
}

impl Lexer<StrReader> {
    pub fn from_str(code: &'static str) -> Lexer<StrReader> {
        Lexer::new(StrReader::new(code))
    }
}

impl Lexer<StrReader> {
    pub fn from_file(filename: &'static str) -> Lexer<FileReader> {
        Lexer::new(FileReader::new(filename))
    }
}

fn get_keywords() -> HashMap<&'static str,TokenType> {
    let mut kws = HashMap::new();

    kws.insert("fn", TokenType::Fn);
    kws.insert("var", TokenType::Var);
    kws.insert("while", TokenType::While);
    kws.insert("if", TokenType::If);
    kws.insert("else", TokenType::Else);
    kws.insert("loop", TokenType::Loop);
    kws.insert("break", TokenType::Break);
    kws.insert("continue", TokenType::Continue);
    kws.insert("return", TokenType::Return);
    kws.insert("int", TokenType::Int);

    kws
}

impl<T : CodeReader> Lexer<T> {
    pub fn new(reader: T) -> Lexer<T> {
        Lexer::new_with_tabwidth(reader, 4)
    }

    pub fn new_with_tabwidth(reader: T, tabwidth: u32) -> Lexer<T> {
        let mut lexer = Lexer::<T> {
            reader: reader,
            position: Position::new(1, 1),
            tabwidth: tabwidth,
            eof_reached: false,

            buffer: Vec::with_capacity(10),
            keywords: get_keywords()
        };
        lexer.fill_buffer();

        lexer
    }

    pub fn read_token(&mut self) -> Result<Token,ParseError> {
        loop {
            self.skip_white();

            if self.top().is_none() {
                return Ok(Token::new(TokenType::End, self.position));
            }

            if self.is_digit() {
                return self.read_number();

            } else if self.is_comment_start() {
                match self.read_comment() {
                    Some(err) => return Err(err),
                    _ => {}
                }

            } else if self.is_multi_comment_start() {
                match self.read_multi_comment() {
                    Some(err) => return Err(err),
                    _ => {}
                }

            } else if self.is_identifier_start() {
                return self.read_identifier();

            } else if self.is_string() {
                return self.read_string();

            } else {
                let ch = self.top().unwrap().value;

                return Err( ParseError {
                    filename: self.reader.filename().to_string(),
                    position: self.position,
                    code: ErrorCode::UnknownChar,
                    message: format!("unknown character {} (ascii code {}", ch, ch as usize)
                } )
            }
        }
    }

    fn skip_white(&mut self) {
        while self.is_whitespace() {
            self.read_char();
        }
    }


    fn read_comment(&mut self) -> Option<ParseError> {
        while !self.is_eof() && !self.is_newline() {
            self.read_char();
        }

        None
    }

    fn read_multi_comment(&mut self) -> Option<ParseError> {
        let pos = self.top().unwrap().position;

        self.read_char();
        self.read_char();

        while !self.is_eof() && !self.is_multi_comment_end() {
          self.read_char();
        }

        if self.is_eof() {
          return Some(ParseError {
              filename: self.reader.filename().to_string(),
              position: pos,
              code: ErrorCode::UnclosedComment,
              message: "unclosed comment".to_string()
          } );
        }

        self.read_char();
        self.read_char();

        None
    }

    fn read_identifier(&mut self) -> Result<Token,ParseError> {
        let mut tok = self.build_token(TokenType::Identifier);

        while self.is_identifier() {
            let ch = self.read_char().unwrap().value;
            tok.value.push(ch);
        }

        match self.keywords.get(tok.value.as_slice()) {
            Some(toktype) => tok.ttype = *toktype,
            _ => {}
        }

        Ok(tok)
    }

    fn read_string(&mut self) -> Result<Token,ParseError> {
        let mut tok = self.build_token(TokenType::String);

        self.read_char();

        while !self.is_eof() && !self.is_newline() && !self.is_string() {
            let ch = self.read_char().unwrap().value;
            tok.value.push(ch);
        }

        if self.is_string() {
            self.read_char();

            Ok(tok)
        } else {
            Err(ParseError {
              filename: self.reader.filename().to_string(),
              position: tok.position,
              code: ErrorCode::UnclosedString,
              message: "unclosed string".to_string()
          })
        }
    }

    fn read_number(&mut self) -> Result<Token,ParseError> {
        let mut tok = self.build_token(TokenType::Number);

        while self.is_digit() {
            let ch = self.read_char().unwrap().value;
            tok.value.push(ch);
        }

        Ok(tok)
    }

    fn read_char(&mut self) -> Option<CharPos> {
        if self.buffer.len() > 0 {
            let ch = self.buffer.remove(0);
            self.fill_buffer();

            Some(ch)
        } else {
            None
        }
    }

    fn top(&self) -> Option<CharPos> {
        self.at(0)
    }

    fn at(&self, index: usize) -> Option<CharPos> {
        if self.buffer.len() > index {
            Some(self.buffer[index])
        } else {
            None
        }
    }

    fn build_token(&self, ttype: TokenType) -> Token {
        Token::new(ttype, self.top().unwrap().position)
    }

    fn fill_buffer(&mut self) {
        while !self.eof_reached && self.buffer.len() < 10 {
            let ch = self.reader.read_char();

            if ch.is_some() {
                let ch = ch.unwrap();
                self.buffer.push(CharPos { value: ch, position: self.position });

                match ch {
                    '\n' => {
                        self.position.line += 1;
                        self.position.column = 1;
                    },

                    '\t' => {
                        let tabdepth = (self.position.column-1)/self.tabwidth;

                        self.position.column = 1 + self.tabwidth * (tabdepth+1);
                    }

                    _ => self.position.column += 1
                }
            } else {
                self.eof_reached = true;
            }
        }
    }

    fn is_comment_start(&self) -> bool {
        let top = self.top();
        let ntop = self.at(1);

        top.is_some() && top.unwrap().value == '/' &&
            ntop.is_some() && ntop.unwrap().value == '/'
    }

    fn is_multi_comment_start(&self) -> bool {
        let top = self.top();
        let ntop = self.at(1);

        top.is_some() && top.unwrap().value == '/' &&
            ntop.is_some() && ntop.unwrap().value == '*'
    }

    fn is_multi_comment_end(&self) -> bool {
        let top = self.top();
        let ntop = self.at(1);

        top.is_some() && top.unwrap().value == '*' &&
            ntop.is_some() && ntop.unwrap().value == '/'
    }

    fn is_digit(&self) -> bool {
        let top = self.top();

        top.is_some() && top.unwrap().value.is_digit(10)
    }

    fn is_whitespace(&self) -> bool {
        let top = self.top();

        top.is_some() && top.unwrap().value.is_whitespace()
    }

    fn is_identifier_start(&self) -> bool {
        let top = self.top();
        if top.is_none() { return false; }

        let ch = top.unwrap().value;

        ( ch >= 'a' && ch <= 'z' ) || ( ch >= 'A' && ch <= 'Z' ) || ch == '_'
    }

    fn is_identifier(&self) -> bool {
        self.is_identifier_start() || self.is_digit()
    }

    fn is_newline(&self) -> bool {
        let top = self.top();

        top.is_some() && top.unwrap().value == '\n'
    }

    fn is_string(&self) -> bool {
        let top = self.top();

        top.is_some() && top.unwrap().value == '\"'
    }

    fn is_eof(&self) -> bool {
        self.top().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::reader::StrReader;
    use lexer::token::TokenType;
    use error::ErrorCode;

    fn assert_end(reader: &mut Lexer<StrReader>, l: u32, c: u32) {
        assert_tok(reader, TokenType::End, "", l, c);
    }

    fn assert_tok(reader: &mut Lexer<StrReader>, ttype: TokenType, val: &'static str, l: u32, c: u32) {
        let tok = reader.read_token().unwrap();
        assert_eq!(ttype, tok.ttype);
        assert_eq!(val, tok.value);
        assert_eq!(l, tok.position.line);
        assert_eq!(c, tok.position.column);
    }

    fn assert_err(reader: &mut Lexer<StrReader>, code: ErrorCode, l: u32, c: u32) {
        let err = reader.read_token().unwrap_err();
        assert_eq!(code, err.code);
        assert_eq!(l, err.position.line);
        assert_eq!(c, err.position.column);
    }

    #[test]
    fn test_read_empty_file() {
        let mut reader = Lexer::from_str("");
        assert_end(&mut reader, 1, 1);
        assert_end(&mut reader, 1, 1);
    }

    #[test]
    fn test_read_numbers() {
        let mut reader = Lexer::from_str("1 2\n0123 10");
        assert_tok(&mut reader, TokenType::Number, "1", 1, 1);
        assert_tok(&mut reader, TokenType::Number, "2", 1, 3);
        assert_tok(&mut reader, TokenType::Number, "0123", 2, 1);
        assert_tok(&mut reader, TokenType::Number, "10", 2, 6);
        assert_end(&mut reader, 2, 8);
    }

    #[test]
    fn test_skip_single_line_comment() {
        let mut reader = Lexer::from_str("//test\n1");
        assert_tok(&mut reader, TokenType::Number, "1", 2, 1);
        assert_end(&mut reader, 2, 2);
    }

    #[test]
    fn test_unfinished_line_comment() {
        let mut reader = Lexer::from_str("//abc");
        assert_end(&mut reader, 1, 6);
    }

    #[test]
    fn test_skip_multi_comment() {
        let mut reader = Lexer::from_str("/*test*/1");
        assert_tok(&mut reader, TokenType::Number, "1", 1, 9);
        assert_end(&mut reader, 1, 10);
    }

    #[test]
    fn test_unfinished_multi_comment() {
        let mut reader = Lexer::from_str("/*test");
        assert_err(&mut reader, ErrorCode::UnclosedComment, 1, 1);

        let mut reader = Lexer::from_str("1/*test");
        assert_tok(&mut reader, TokenType::Number, "1", 1, 1);
        assert_err(&mut reader, ErrorCode::UnclosedComment, 1, 2);
    }

    #[test]
    fn test_read_identifier() {
        let mut reader = Lexer::from_str("abc ident test");
        assert_tok(&mut reader, TokenType::Identifier, "abc", 1, 1);
        assert_tok(&mut reader, TokenType::Identifier, "ident", 1, 5);
        assert_tok(&mut reader, TokenType::Identifier, "test", 1, 11);
        assert_end(&mut reader, 1, 15);

    }

    #[test]
    fn test_code_with_spaces() {
        let mut reader = Lexer::from_str("1 2 3");
        assert_tok(&mut reader, TokenType::Number, "1", 1, 1);
        assert_tok(&mut reader, TokenType::Number, "2", 1, 3);
        assert_tok(&mut reader, TokenType::Number, "3", 1, 5);
        assert_end(&mut reader, 1, 6);
    }

    #[test]
    fn test_code_with_newlines() {
        let mut reader = Lexer::from_str("1\n2\n3");
        assert_tok(&mut reader, TokenType::Number, "1", 1, 1);
        assert_tok(&mut reader, TokenType::Number, "2", 2, 1);
        assert_tok(&mut reader, TokenType::Number, "3", 3, 1);
        assert_end(&mut reader, 3, 2);
    }

    #[test]
    fn test_code_with_tabs() {
        let mut reader = Lexer::from_str("1\t2\t3");
        assert_tok(&mut reader, TokenType::Number, "1", 1, 1);
        assert_tok(&mut reader, TokenType::Number, "2", 1, 5);
        assert_tok(&mut reader, TokenType::Number, "3", 1, 9);
        assert_end(&mut reader, 1, 10);
    }

    #[test]
    fn test_code_with_tabwidth8() {
        let mut str_reader = StrReader::new("1\t2\n1234567\t8\n12345678\t9");
        let mut reader = Lexer::new_with_tabwidth(str_reader, 8);

        assert_tok(&mut reader, TokenType::Number, "1", 1, 1);
        assert_tok(&mut reader, TokenType::Number, "2", 1, 9);
        assert_tok(&mut reader, TokenType::Number, "1234567", 2, 1);
        assert_tok(&mut reader, TokenType::Number, "8", 2, 9);
        assert_tok(&mut reader, TokenType::Number, "12345678", 3, 1);
        assert_tok(&mut reader, TokenType::Number, "9", 3, 17);
        assert_end(&mut reader, 3, 18);
    }

    #[test]
    fn test_string_with_newline() {
        let mut reader = Lexer::from_str("\"abc\ndef\"");
        assert_err(&mut reader, ErrorCode::UnclosedString, 1, 1);
    }

    #[test]
    fn test_unclosed_string() {
        let mut reader = Lexer::from_str("\"abc");
        assert_err(&mut reader, ErrorCode::UnclosedString, 1, 1);
    }

    #[test]
    fn test_string() {
        let mut reader = Lexer::from_str("\"abc\"");
        assert_tok(&mut reader, TokenType::String, "abc", 1, 1);
        assert_end(&mut reader, 1, 6);
    }

    #[test]
    fn test_keywords() {
        let mut reader = Lexer::from_str("fn var while if else");
        assert_tok(&mut reader, TokenType::Fn, "fn", 1, 1);
        assert_tok(&mut reader, TokenType::Var, "var", 1, 4);
        assert_tok(&mut reader, TokenType::While, "while", 1, 8);
        assert_tok(&mut reader, TokenType::If, "if", 1, 14);
        assert_tok(&mut reader, TokenType::Else, "else", 1, 17);

        let mut reader = Lexer::from_str("loop break continue return int");
        assert_tok(&mut reader, TokenType::Loop, "loop", 1, 1);
        assert_tok(&mut reader, TokenType::Break, "break", 1, 6);
        assert_tok(&mut reader, TokenType::Continue, "continue", 1, 12);
        assert_tok(&mut reader, TokenType::Return, "return", 1, 21);
        assert_tok(&mut reader, TokenType::Int, "int", 1, 28);
    }
}

