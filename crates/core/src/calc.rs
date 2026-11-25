//! Calculator and length-unit conversion for the search field.
//!
//! Expressions are parsed with a small recursive-descent parser rather than
//! evaluated dynamically. Unit conversion is factor-based and uses metres as
//! the base unit, so every supported length unit stays consistent.

/// What kind of result the card represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcKind {
    Math,
    Unit,
}

/// A calculation shown above the app results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcResult {
    /// Icon/category hint for the UI.
    pub kind: CalcKind,
    /// What the user typed, trimmed for display.
    pub expression: String,
    /// Main result, for example `4` or `25.4 mm`.
    pub primary: String,
    /// Secondary conversions shown under the main result.
    pub detail: String,
    /// Text placed on the clipboard when the card is activated.
    pub copy_text: String,
}

/// Evaluates a query as a unit conversion or a math expression.
///
/// Plain numbers are deliberately not treated as calculations so apps such as
/// `7-Zip` or `1Password` still search normally.
pub fn evaluate(input: &str) -> Option<CalcResult> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return None;
    }

    if let Some(conversion) = convert_units(trimmed) {
        return Some(conversion);
    }

    if is_plain_number(trimmed) || !looks_like_math(trimmed) {
        return None;
    }

    let value = evaluate_math(trimmed)?;
    let primary = format_number(value);

    Some(CalcResult {
        kind: CalcKind::Math,
        expression: trimmed.to_owned(),
        primary: primary.clone(),
        detail: String::new(),
        copy_text: primary,
    })
}

fn looks_like_math(text: &str) -> bool {
    text.chars().any(|character| character.is_ascii_digit())
        || text.contains('(')
        || text
            .chars()
            .any(|character| matches!(character, '+' | '*' | '/' | '^'))
}

fn is_plain_number(text: &str) -> bool {
    text.parse::<f64>().is_ok()
}

fn evaluate_math(input: &str) -> Option<f64> {
    let mut parser = Parser::new(input);
    let value = parser.parse_expression()?;

    parser.skip_whitespace();

    if parser.pos != parser.characters.len() || !value.is_finite() {
        return None;
    }

    Some(value)
}

struct Parser {
    characters: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            characters: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek();
        if character.is_some() {
            self.pos += 1;
        }
        character
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn parse_expression(&mut self) -> Option<f64> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Option<f64> {
        let mut value = self.parse_multiplicative()?;

        loop {
            self.skip_whitespace();

            match self.peek() {
                Some('+') => {
                    self.bump();
                    value += self.parse_multiplicative()?;
                }
                Some('-') => {
                    self.bump();
                    value -= self.parse_multiplicative()?;
                }
                _ => break,
            }
        }

        Some(value)
    }

    fn parse_multiplicative(&mut self) -> Option<f64> {
        let mut value = self.parse_unary()?;

        loop {
            self.skip_whitespace();

            match self.peek() {
                Some('*') => {
                    self.bump();
                    value *= self.parse_unary()?;
                }
                Some('/') => {
                    self.bump();
                    let divisor = self.parse_unary()?;

                    if divisor == 0.0 {
                        return None;
                    }

                    value /= divisor;
                }
                _ => break,
            }
        }

        Some(value)
    }

    fn parse_unary(&mut self) -> Option<f64> {
        self.skip_whitespace();

        match self.peek() {
            Some('+') => {
                self.bump();
                self.parse_unary()
            }
            Some('-') => {
                self.bump();
                self.parse_unary().map(|value| -value)
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Option<f64> {
        let base = self.parse_primary()?;
        self.skip_whitespace();

        if self.peek() == Some('^') {
            self.bump();
            let exponent = self.parse_unary()?;
            Some(base.powf(exponent))
        } else {
            Some(base)
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_whitespace();

        match self.peek()? {
            character if character.is_ascii_digit() || character == '.' => self.parse_number(),
            '(' => {
                self.bump();
                let value = self.parse_expression()?;
                self.skip_whitespace();

                if self.bump() != Some(')') {
                    return None;
                }

                Some(value)
            }
            character if character.is_ascii_alphabetic() => {
                let start = self.pos;

                while self.peek().is_some_and(|next| next.is_ascii_alphabetic()) {
                    self.pos += 1;
                }

                let name: String = self.characters[start..self.pos]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase();
                self.skip_whitespace();

                if self.peek() == Some('(') {
                    self.bump();
                    let argument = self.parse_expression()?;
                    self.skip_whitespace();

                    if self.bump() != Some(')') {
                        return None;
                    }

                    apply_function(&name, argument)
                } else {
                    constant(&name)
                }
            }
            _ => None,
        }
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.pos;
        let mut has_digit = false;
        let mut has_dot = false;
        let mut has_exponent = false;

        while let Some(character) = self.peek() {
            match character {
                '0'..='9' => {
                    has_digit = true;
                    self.pos += 1;
                }
                '.' if !has_dot && !has_exponent => {
                    has_dot = true;
                    self.pos += 1;
                }
                'e' | 'E' if has_digit && !has_exponent => {
                    has_exponent = true;
                    self.pos += 1;

                    if matches!(self.peek(), Some('+') | Some('-')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }

        if !has_digit {
            return None;
        }

        let mut text: String = self.characters[start..self.pos].iter().collect();

        if text.ends_with('.') {
            text.push('0');
        }

        text.parse::<f64>().ok()
    }
}

fn constant(name: &str) -> Option<f64> {
    match name {
        "pi" => Some(std::f64::consts::PI),
        "e" => Some(std::f64::consts::E),
        _ => None,
    }
}

fn apply_function(name: &str, argument: f64) -> Option<f64> {
    match name {
        "sqrt" if argument >= 0.0 => Some(argument.sqrt()),
        "abs" => Some(argument.abs()),
        "round" => Some(argument.round()),
        "floor" => Some(argument.floor()),
        "ceil" => Some(argument.ceil()),
        "sin" => Some(argument.sin()),
        "cos" => Some(argument.cos()),
        "tan" => Some(argument.tan()),
        "ln" if argument > 0.0 => Some(argument.ln()),
        "log" if argument > 0.0 => Some(argument.log10()),
        _ => None,
    }
}

struct Unit {
    key: &'static str,
    label: &'static str,
    metres: f64,
}

const UNITS: &[Unit] = &[
    Unit {
        key: "mm",
        label: "mm",
        metres: 0.001,
    },
    Unit {
        key: "cm",
        label: "cm",
        metres: 0.01,
    },
    Unit {
        key: "m",
        label: "m",
        metres: 1.0,
    },
    Unit {
        key: "km",
        label: "km",
        metres: 1_000.0,
    },
    Unit {
        key: "in",
        label: "in",
        metres: 0.0254,
    },
    Unit {
        key: "ft",
        label: "ft",
        metres: 0.3048,
    },
    Unit {
        key: "yd",
        label: "yd",
        metres: 0.9144,
    },
    Unit {
        key: "mi",
        label: "mi",
        metres: 1_609.344,
    },
];

const ORDER_MM: &[&str] = &["in", "cm", "m", "ft", "yd", "mi", "km"];
const ORDER_CM: &[&str] = &["in", "m", "mm", "ft", "yd", "mi", "km"];
const ORDER_M: &[&str] = &["ft", "cm", "mm", "in", "yd", "mi", "km"];
const ORDER_KM: &[&str] = &["mi", "m", "ft", "yd", "in", "cm", "mm"];
const ORDER_IN: &[&str] = &["mm", "cm", "m", "ft", "yd", "mi", "km"];
const ORDER_FT: &[&str] = &["m", "cm", "mm", "in", "yd", "mi", "km"];
const ORDER_YD: &[&str] = &["m", "cm", "mm", "ft", "in", "mi", "km"];
const ORDER_MI: &[&str] = &["km", "m", "ft", "yd", "in", "cm", "mm"];

fn unit_order(key: &str) -> &'static [&'static str] {
    match key {
        "mm" => ORDER_MM,
        "cm" => ORDER_CM,
        "m" => ORDER_M,
        "km" => ORDER_KM,
        "in" => ORDER_IN,
        "ft" => ORDER_FT,
        "yd" => ORDER_YD,
        "mi" => ORDER_MI,
        _ => ORDER_MM,
    }
}

fn find_unit(name: &str) -> Option<&'static Unit> {
    let name = name.trim().to_ascii_lowercase().replace(' ', "");
    let key = match name.as_str() {
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => "mm",
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => "cm",
        "m" | "meter" | "meters" | "metre" | "metres" => "m",
        "km" | "kilometer" | "kilometers" | "kilometre" | "kilometres" => "km",
        "in" | "inch" | "inches" | "\"" => "in",
        "ft" | "foot" | "feet" | "'" => "ft",
        "yd" | "yard" | "yards" => "yd",
        "mi" | "mile" | "miles" => "mi",
        _ => return None,
    };

    find_unit_by_key(key)
}

fn find_unit_by_key(key: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|unit| unit.key == key)
}

fn convert_units(input: &str) -> Option<CalcResult> {
    let normalized = input
        .to_lowercase()
        .replace("->", " to ")
        .replace('→', " to ");
    let normalized = normalized.trim();
    let (quantity_text, target_text) = match normalized.split_once(" to ") {
        Some((quantity, target)) => (quantity.trim(), Some(target.trim())),
        None => (normalized, None),
    };
    let (value, unit_text) = split_quantity(quantity_text)?;
    let from = find_unit(unit_text)?;
    let base = value * from.metres;

    if !base.is_finite() {
        return None;
    }

    if let Some(target_text) = target_text {
        let target = find_unit(target_text)?;
        let converted = base / target.metres;

        if !converted.is_finite() {
            return None;
        }

        let primary = format_unit(converted, target);

        return Some(CalcResult {
            kind: CalcKind::Unit,
            expression: input.trim().to_owned(),
            primary: primary.clone(),
            detail: String::new(),
            copy_text: primary,
        });
    }

    let mut conversions = Vec::new();

    for key in unit_order(from.key) {
        if *key == from.key {
            continue;
        }

        let Some(unit) = find_unit_by_key(key) else {
            continue;
        };
        let converted = base / unit.metres;

        if converted.is_finite() {
            conversions.push(format_unit(converted, unit));
        }
    }

    let primary = conversions.first()?.clone();
    let detail = conversions
        .iter()
        .skip(1)
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    let copy_text = conversions.join("\n");

    Some(CalcResult {
        kind: CalcKind::Unit,
        expression: input.trim().to_owned(),
        primary,
        detail,
        copy_text,
    })
}

fn split_quantity(input: &str) -> Option<(f64, &str)> {
    let input = input.trim();
    let mut end = 0;
    let mut has_digit = false;
    let mut has_dot = false;
    let mut has_exponent = false;
    let mut exponent_sign_allowed = false;

    for (index, character) in input.char_indices() {
        match character {
            '0'..='9' => {
                has_digit = true;
                exponent_sign_allowed = false;
                end = index + character.len_utf8();
            }
            '.' if !has_dot && !has_exponent => {
                has_dot = true;
                end = index + character.len_utf8();
            }
            '+' | '-' if index == 0 || exponent_sign_allowed => {
                exponent_sign_allowed = false;
                end = index + character.len_utf8();
            }
            'e' | 'E' if has_digit && !has_exponent => {
                has_exponent = true;
                exponent_sign_allowed = true;
                end = index + character.len_utf8();
            }
            _ => break,
        }
    }

    if !has_digit {
        return None;
    }

    let mut number = input[..end].to_owned();

    if number.ends_with('.') {
        number.push('0');
    }

    let value = number.parse::<f64>().ok()?;
    let unit = input[end..].trim();

    Some((value, unit))
}

fn format_unit(value: f64, unit: &Unit) -> String {
    format!("{} {}", format_number(value), unit.label)
}

fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "—".to_owned();
    }

    if value == 0.0 {
        return "0".to_owned();
    }

    // Floating-point functions such as sin(pi) produce tiny residues that are
    // noise for a launcher result.
    if value.abs() < 1e-12 {
        return "0".to_owned();
    }

    let magnitude = value.abs();

    if !(1e-6..1e12).contains(&magnitude) {
        let text = format!("{value:.8e}");

        if let Some(index) = text.find('e') {
            let (mantissa, exponent) = text.split_at(index);
            let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
            return format!("{mantissa}{exponent}");
        }

        return text;
    }

    let mut text = format!("{value:.10}");

    while text.ends_with('0') {
        text.pop();
    }

    if text.ends_with('.') {
        text.pop();
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn math(input: &str) -> String {
        evaluate(input).expect("expression should evaluate").primary
    }

    #[test]
    fn evaluates_basic_math() {
        assert_eq!(math("2 + 2"), "4");
        assert_eq!(math("2 * (3 + 4)"), "14");
        assert_eq!(math("(2 + 3) * 4"), "20");
        assert_eq!(math("2^3^2"), "512");
        assert_eq!(math("-2^2"), "-4");
        assert_eq!(math("2^-2"), "0.25");
        assert_eq!(math("sqrt(16)"), "4");
        assert_eq!(math("sin(pi)"), "0");
        assert_eq!(math("cos(0)"), "1");
    }

    #[test]
    fn rounds_floating_point_noise() {
        assert_eq!(math("0.1 + 0.2"), "0.3");
        assert_eq!(math("1 / 3"), "0.3333333333");
    }

    #[test]
    fn rejects_invalid_or_plain_math() {
        assert!(evaluate("42").is_none());
        assert!(evaluate("2 +").is_none());
        assert!(evaluate("2 / 0").is_none());
        assert!(evaluate("sqrt(-1)").is_none());
        assert!(evaluate("abc").is_none());
    }

    #[test]
    fn converts_inches_to_millimetres() {
        let result = evaluate("1 inch").expect("unit conversion should work");

        assert_eq!(result.kind, CalcKind::Unit);
        assert_eq!(result.primary, "25.4 mm");
        assert!(result.detail.contains("2.54 cm"));
        assert!(result.detail.contains("0.0254 m"));
        assert!(result.copy_text.contains("25.4 mm"));
    }

    #[test]
    fn converts_to_an_explicit_target() {
        assert_eq!(
            evaluate("1 inch to cm")
                .expect("target conversion should work")
                .primary,
            "2.54 cm"
        );
        assert_eq!(
            evaluate("1 inch -> mm")
                .expect("arrow conversion should work")
                .primary,
            "25.4 mm"
        );
    }

    #[test]
    fn accepts_aliases_and_attached_units() {
        assert_eq!(
            evaluate("25.4mm")
                .expect("attached unit should work")
                .primary,
            "1 in"
        );
        assert_eq!(
            evaluate("1 foot").expect("foot alias should work").primary,
            "0.3048 m"
        );
        assert_eq!(
            evaluate("1 m to ft")
                .expect("metre conversion should work")
                .primary,
            "3.280839895 ft"
        );
    }

    #[test]
    fn rejects_unknown_units() {
        assert!(evaluate("1 lightyear").is_none());
        assert!(evaluate("1 inch to bananas").is_none());
    }
}
