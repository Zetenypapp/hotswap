//! Minimal parser for Valve's text KeyValues format (.vdf / .acf).

pub enum Node {
    Str(String),
    Map(Vec<(String, Node)>),
}

impl Node {
    /// Case-insensitive child lookup.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Node::Str(_) => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Node> {
        keys.iter().try_fold(self, |node, k| node.get(k))
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Node::Str(s) => Some(s),
            Node::Map(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Node)] {
        match self {
            Node::Map(items) => items,
            Node::Str(_) => &[],
        }
    }
}

pub fn parse(text: &str) -> Node {
    let mut tokens = Tokens { chars: text.chars().peekable() };
    Node::Map(parse_map(&mut tokens))
}

struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

enum Tok {
    Str(String),
    Open,
    Close,
}

impl Iterator for Tokens<'_> {
    type Item = Tok;
    fn next(&mut self) -> Option<Tok> {
        loop {
            let c = self.chars.next()?;
            match c {
                '{' => return Some(Tok::Open),
                '}' => return Some(Tok::Close),
                '"' => {
                    let mut s = String::new();
                    while let Some(c) = self.chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match self.chars.next() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(other) => s.push(other),
                                None => break,
                            },
                            _ => s.push(c),
                        }
                    }
                    return Some(Tok::Str(s));
                }
                '/' if self.chars.peek() == Some(&'/') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                c if c.is_whitespace() => {}
                _ => {
                    // Unquoted token
                    let mut s = String::from(c);
                    while let Some(&n) = self.chars.peek() {
                        if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                            break;
                        }
                        s.push(n);
                        self.chars.next();
                    }
                    return Some(Tok::Str(s));
                }
            }
        }
    }
}

fn parse_map(tokens: &mut Tokens) -> Vec<(String, Node)> {
    let mut items = Vec::new();
    while let Some(tok) = tokens.next() {
        let key = match tok {
            Tok::Str(k) => k,
            Tok::Close => break,
            Tok::Open => continue,
        };
        match tokens.next() {
            Some(Tok::Str(v)) => items.push((key, Node::Str(v))),
            Some(Tok::Open) => items.push((key, Node::Map(parse_map(tokens)))),
            _ => break,
        }
    }
    items
}
