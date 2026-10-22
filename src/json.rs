// SPDX-License-Identifier: GPL-3.0-or-later

//! A small JSON reader for exactly what the install script prints: objects, arrays, strings,
//! whole numbers, booleans and null. It is strict where it is cheap to be: a key twice, a control
//! character in a string, a lone surrogate, trailing text, a number that is not a plain whole
//! number, and nesting deeper than [`MAX_DEPTH`] are errors. The text is never copied into an
//! error message.

/// The deepest nesting accepted. The script's output is three levels deep.
pub const MAX_DEPTH: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    /// A whole number that fits in 64 bits, without a sign, a fraction or an exponent.
    Int(u64),
    /// Any other number: valid JSON, but not one the script prints.
    Other,
    Str(String),
    Arr(Vec<Json>),
    /// In the order of the text; no key is twice.
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(v) => Some(v),
            _ => None,
        }
    }
    /// The number of fields of an object.
    pub fn len(&self) -> usize {
        match self {
            Json::Obj(v) => v.len(),
            Json::Arr(v) => v.len(),
            _ => 0,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Why the text is not JSON this reader accepts. The place, never the text.
#[derive(Debug, PartialEq, Eq)]
pub struct Error(pub &'static str, pub usize);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.0, self.1)
    }
}

pub fn parse(text: &str) -> Result<Json, Error> {
    let mut p = P {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return Err(p.err("text after the value"));
    }
    Ok(v)
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn err(&self, what: &'static str) -> Error {
        Error(what, self.i)
    }
    fn ws(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        let ok = self.b.get(self.i) == Some(&c);
        self.i += usize::from(ok);
        ok
    }
    fn word(&mut self, w: &str, v: Json) -> Result<Json, Error> {
        if self.b[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            Err(self.err("not a JSON value"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, Error> {
        if depth > MAX_DEPTH {
            return Err(self.err("nested too deeply"));
        }
        match self.b.get(self.i) {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("not a JSON value")),
            None => Err(self.err("the text ends early")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, Error> {
        self.i += 1;
        let mut fields: Vec<(String, Json)> = Vec::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Json::Obj(fields));
        }
        loop {
            self.ws();
            if self.b.get(self.i) != Some(&b'"') {
                return Err(self.err("a key was expected"));
            }
            let key = self.string()?;
            if fields.iter().any(|(k, _)| *k == key) {
                return Err(self.err("a key twice"));
            }
            self.ws();
            if !self.eat(b':') {
                return Err(self.err("a colon was expected"));
            }
            self.ws();
            let v = self.value(depth + 1)?;
            fields.push((key, v));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Json::Obj(fields));
            }
            return Err(self.err("a comma or } was expected"));
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, Error> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value(depth + 1)?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Json::Arr(items));
            }
            return Err(self.err("a comma or ] was expected"));
        }
    }

    fn number(&mut self) -> Result<Json, Error> {
        let start = self.i;
        self.eat(b'-');
        let digits = self.i;
        while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        if self.i == digits || (self.b[digits] == b'0' && self.i - digits > 1) {
            return Err(self.err("not a number"));
        }
        let mut whole = self.b[start] != b'-';
        if self.eat(b'.') {
            whole = false;
            let f = self.i;
            while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
            if self.i == f {
                return Err(self.err("not a number"));
            }
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            whole = false;
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            let e = self.i;
            while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
            if self.i == e {
                return Err(self.err("not a number"));
            }
        }
        if !whole {
            return Ok(Json::Other);
        }
        // Only ASCII digits were read, so this is valid UTF-8.
        let n = std::str::from_utf8(&self.b[start..self.i])
            .ok()
            .and_then(|s| s.parse::<u64>().ok());
        Ok(n.map_or(Json::Other, Json::Int))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let digits = self
            .b
            .get(self.i..self.i + 4)
            .ok_or(self.err("\\u is cut"))?;
        let mut n = 0;
        for &d in digits {
            let v = (d as char).to_digit(16).ok_or(self.err("bad \\u escape"))?;
            n = n * 16 + v;
        }
        self.i += 4;
        Ok(n)
    }

    fn string(&mut self) -> Result<String, Error> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            // A run of ordinary characters; the input is a `&str`, so the bytes are valid UTF-8.
            while self
                .b
                .get(self.i)
                .is_some_and(|&c| c != b'"' && c != b'\\' && c >= 0x20)
            {
                self.i += 1;
            }
            if let Ok(run) = std::str::from_utf8(&self.b[start..self.i]) {
                out.push_str(run);
            }
            match self.b.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let c = *self.b.get(self.i).ok_or(self.err("the text ends early"))?;
                    self.i += 1;
                    out.push(match c {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return Err(self.err("bad escape")),
                    });
                }
                Some(_) => return Err(self.err("a control character in a string")),
                None => return Err(self.err("the text ends early")),
            }
        }
    }

    /// After `\u`: one character, or a surrogate pair written as two escapes.
    fn unicode(&mut self) -> Result<char, Error> {
        let first = self.hex4()?;
        let code = match first {
            0xd800..=0xdbff => {
                if !(self.eat(b'\\') && self.eat(b'u')) {
                    return Err(self.err("a lone surrogate"));
                }
                let second = self.hex4()?;
                if !(0xdc00..=0xdfff).contains(&second) {
                    return Err(self.err("a lone surrogate"));
                }
                0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
            }
            0xdc00..=0xdfff => return Err(self.err("a lone surrogate")),
            n => n,
        };
        char::from_u32(code).ok_or(self.err("not a character"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Json {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }

    #[test]
    fn reads_what_the_script_prints() {
        let v = ok(
            r#" {"a": [1, 2, {"b": null}], "c": "x\ty\u00e9\ud83d\ude00", "d": true, "e": 18446744073709551615} "#,
        );
        assert_eq!(v.get("a").unwrap().len(), 3);
        assert_eq!(v.get("c").unwrap().as_str(), Some("x\tyé😀"));
        assert_eq!(v.get("d").unwrap().as_bool(), Some(true));
        assert_eq!(v.get("e").unwrap().as_u64(), Some(u64::MAX));
        assert_eq!(ok("[]"), Json::Arr(vec![]));
        assert_eq!(ok("{}").len(), 0);
    }

    #[test]
    fn numbers_that_are_not_whole_are_other() {
        for n in ["1.5", "-1", "1e3", "18446744073709551616", "0.0"] {
            assert_eq!(ok(n), Json::Other, "{n}");
        }
        assert_eq!(ok("0"), Json::Int(0));
    }

    #[test]
    fn refuses_what_is_not_json_or_not_plain() {
        let bad = [
            "",
            " ",
            "{",
            "{}x",
            "[1,]",
            "[,1]",
            "{\"a\":1,}",
            "{\"a\" 1}",
            "{a:1}",
            "{\"a\":1,\"a\":2}",
            "\"a\nb\"",
            "\"a\u{1}b\"",
            "\"\\x\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "\"\\ud800\\u0041\"",
            "\"\\u12\"",
            "\"abc",
            "01",
            "-",
            "1.",
            "1e",
            "tru",
            "nul",
            "'a'",
            "{} {}",
            "[[[[[[[[[[1]]]]]]]]]]",
        ];
        for text in bad {
            assert!(parse(text).is_err(), "{text:?} was accepted");
        }
    }

    #[test]
    fn the_error_never_holds_the_text() {
        let e = parse("{\"secret-password-1234\": nope}").unwrap_err();
        assert!(!e.to_string().contains("secret"));
    }

    #[test]
    fn random_bytes_never_panic() {
        // A fixed seed, so a failure can be repeated.
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let alphabet = b"{}[]\":,\\ntfu0123456789.eE+- abcdef\x01\xc3\xa9";
        for _ in 0..20_000 {
            let len = (next() % 40) as usize;
            let bytes: Vec<u8> = (0..len)
                .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
                .collect();
            if let Ok(s) = std::str::from_utf8(&bytes) {
                let _ = parse(s);
            }
        }
    }
}
