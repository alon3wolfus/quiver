//! A small recursive-descent calculator for the launcher.
//! Supports + - * / % ^, parentheses, pi, e and a few functions like sqrt(x).

pub fn eval(input: &str) -> Option<f64> {
    let tokens: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();
    let mut p = Parser { tokens, pos: 0 };
    let value = p.expr()?;
    (p.pos == p.tokens.len() && value.is_finite()).then_some(value)
}

/// Formats a result without float noise, e.g. 0.1+0.2 shows as 0.3.
pub fn format(value: f64) -> String {
    let rounded = (value * 1e10).round() / 1e10;
    if rounded.fract() == 0.0 && rounded.abs() < 1e15 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

struct Parser {
    tokens: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.tokens.get(self.pos).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        if found {
            self.pos += 1;
        }
        found
    }

    // expr = term (('+' | '-') term)*
    fn expr(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        loop {
            if self.eat('+') {
                value += self.term()?;
            } else if self.eat('-') {
                value -= self.term()?;
            } else {
                return Some(value);
            }
        }
    }

    // term = power (('*' | '/' | '%') power)*
    fn term(&mut self) -> Option<f64> {
        let mut value = self.power()?;
        loop {
            if self.eat('*') {
                value *= self.power()?;
            } else if self.eat('/') {
                value /= self.power()?;
            } else if self.eat('%') {
                value %= self.power()?;
            } else {
                return Some(value);
            }
        }
    }

    // power = unary ('^' power)?   (right-associative: 2^3^2 = 2^9)
    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.eat('^') {
            return Some(base.powf(self.power()?));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat('-') {
            return Some(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        if self.eat('(') {
            let value = self.expr()?;
            return self.eat(')').then_some(value);
        }
        let start = self.pos;
        if self.peek()?.is_ascii_alphabetic() {
            while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
                self.pos += 1;
            }
            let name: String = self.tokens[start..self.pos].iter().collect();
            return match name.as_str() {
                "pi" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                _ => {
                    let arg = self.atom()?;
                    match name.as_str() {
                        "sqrt" => Some(arg.sqrt()),
                        "abs" => Some(arg.abs()),
                        "sin" => Some(arg.sin()),
                        "cos" => Some(arg.cos()),
                        "tan" => Some(arg.tan()),
                        "ln" => Some(arg.ln()),
                        "log" => Some(arg.log10()),
                        "round" => Some(arg.round()),
                        "floor" => Some(arg.floor()),
                        "ceil" => Some(arg.ceil()),
                        _ => None,
                    }
                }
            };
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        let number: String = self.tokens[start..self.pos].iter().collect();
        number.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_parens() {
        assert_eq!(eval("2+3*4"), Some(14.0));
        assert_eq!(eval("(2+3)*4"), Some(20.0));
        assert_eq!(eval("2^3^2"), Some(512.0));
        assert_eq!(eval("-2^2"), Some(4.0));
        assert_eq!(eval("10 % 4"), Some(2.0));
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(eval("sqrt(16)"), Some(4.0));
        assert_eq!(format(eval("cos(pi)").unwrap()), "-1");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(eval("2+"), None);
        assert_eq!(eval("firefox"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("(1"), None);
    }

    #[test]
    fn formats_cleanly() {
        assert_eq!(format(eval("0.1+0.2").unwrap()), "0.3");
        assert_eq!(format(2.5), "2.5");
    }
}
