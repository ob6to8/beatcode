//! Minimal JSON parser for reading the golden vector files in tests.
//! Panics on malformed input (test context). Numbers are kept as raw
//! text so callers can parse u64 keys exactly and compare floats by
//! bit pattern.
#![allow(dead_code)] // each test target uses a different subset

#[derive(Clone, Debug)]
pub enum Json {
    Obj(Vec<(String, Json)>),
    Arr(Vec<Json>),
    Str(String),
    Num(String),
    Bool(bool),
    Null,
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kvs) => kvs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn s(&self) -> &str {
        match self {
            Json::Str(s) => s,
            other => panic!("expected string, got {other:?}"),
        }
    }

    pub fn raw(&self) -> &str {
        match self {
            Json::Num(s) => s,
            other => panic!("expected number, got {other:?}"),
        }
    }

    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            other => panic!("expected array, got {other:?}"),
        }
    }
}

pub fn parse(s: &str) -> Json {
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let v = value(&b, &mut i);
    skip_ws(&b, &mut i);
    assert!(i == b.len(), "trailing JSON garbage at {i}");
    v
}

fn skip_ws(b: &[char], i: &mut usize) {
    while *i < b.len() && b[*i].is_whitespace() {
        *i += 1;
    }
}

fn value(b: &[char], i: &mut usize) -> Json {
    skip_ws(b, i);
    match b[*i] {
        '{' => {
            *i += 1;
            let mut kvs = Vec::new();
            skip_ws(b, i);
            if b[*i] == '}' {
                *i += 1;
                return Json::Obj(kvs);
            }
            loop {
                skip_ws(b, i);
                let k = string(b, i);
                skip_ws(b, i);
                assert!(b[*i] == ':', "expected ':'");
                *i += 1;
                kvs.push((k, value(b, i)));
                skip_ws(b, i);
                match b[*i] {
                    ',' => *i += 1,
                    '}' => {
                        *i += 1;
                        return Json::Obj(kvs);
                    }
                    c => panic!("expected ',' or '}}', got {c:?}"),
                }
            }
        }
        '[' => {
            *i += 1;
            let mut vs = Vec::new();
            skip_ws(b, i);
            if b[*i] == ']' {
                *i += 1;
                return Json::Arr(vs);
            }
            loop {
                vs.push(value(b, i));
                skip_ws(b, i);
                match b[*i] {
                    ',' => *i += 1,
                    ']' => {
                        *i += 1;
                        return Json::Arr(vs);
                    }
                    c => panic!("expected ',' or ']', got {c:?}"),
                }
            }
        }
        '"' => Json::Str(string(b, i)),
        't' => {
            lit(b, i, "true");
            Json::Bool(true)
        }
        'f' => {
            lit(b, i, "false");
            Json::Bool(false)
        }
        'n' => {
            lit(b, i, "null");
            Json::Null
        }
        _ => {
            let start = *i;
            while *i < b.len() && matches!(b[*i], '0'..='9' | '-' | '+' | '.' | 'e' | 'E') {
                *i += 1;
            }
            Json::Num(b[start..*i].iter().collect())
        }
    }
}

fn lit(b: &[char], i: &mut usize, s: &str) {
    for c in s.chars() {
        assert!(b[*i] == c, "bad literal");
        *i += 1;
    }
}

fn string(b: &[char], i: &mut usize) -> String {
    assert!(b[*i] == '"', "expected string");
    *i += 1;
    let mut out = String::new();
    loop {
        match b[*i] {
            '"' => {
                *i += 1;
                return out;
            }
            '\\' => {
                *i += 1;
                match b[*i] {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' => {
                        let hex: String = b[*i + 1..*i + 5].iter().collect();
                        let cp = u32::from_str_radix(&hex, 16).expect("hex escape");
                        out.push(char::from_u32(cp).expect("BMP scalar"));
                        *i += 4;
                    }
                    c => panic!("unsupported escape {c:?}"),
                }
                *i += 1;
            }
            c => {
                out.push(c);
                *i += 1;
            }
        }
    }
}
