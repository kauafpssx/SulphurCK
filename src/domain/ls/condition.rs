//! Condition evaluation (mirrors RuriLib.Functions.Conditions.Condition).
//!
//! Left may contain `<VAR>` placeholders; Right is a literal (after replace).
//! ANY-of semantics on Left list; numeric compare via f64.

use super::{Comparer, VariableList};

/// Replace `<NAME>` placeholders in `s` from vars; also `<SOURCE>` → response source.
pub fn replace_values(s: &str, vars: &VariableList, source: &str) -> String {
    let mut out = s.to_string();
    // Built-ins first
    out = out.replace("<SOURCE>", source);
    if let Some(addr) = vars.get("ADDRESS") {
        out = out.replace("<ADDRESS>", addr);
    }
    if let Some(rc) = vars.get("RESPONSECODE") {
        out = out.replace("<RESPONSECODE>", rc);
    }
    // Generic <VARNAME>
    for v in &vars.all {
        let token = format!("<{}>", v.name);
        if out.contains(&token) {
            out = out.replace(&token, &v.value);
        }
    }
    out
}

/// Evaluate one condition. Left may be multi-valued (semicolon/newline) — ANY match.
pub fn evaluate(
    left_raw: &str,
    comparer: Comparer,
    right_raw: &str,
    vars: &VariableList,
    source: &str,
) -> bool {
    let left = replace_values(left_raw, vars, source);
    let right = replace_values(right_raw, vars, source);

    match comparer {
        Comparer::Exists => !left.is_empty(),
        Comparer::DoesNotExist => left.is_empty(),
        Comparer::Contains | Comparer::DoesNotContain => {
            let hit = left.contains(&right);
            if comparer == Comparer::Contains {
                hit
            } else {
                !hit
            }
        }
        Comparer::MatchesRegex | Comparer::DoesNotMatchRegex => {
            let re = match regex::Regex::new(&right) {
                Ok(r) => r,
                Err(_) => return false,
            };
            let hit = re.is_match(&left);
            if comparer == Comparer::MatchesRegex {
                hit
            } else {
                !hit
            }
        }
        Comparer::EqualTo
        | Comparer::NotEqualTo
        | Comparer::GreaterThan
        | Comparer::GreaterOrEqual
        | Comparer::LessThan
        | Comparer::LessThanOrEqual => {
            // Try numeric first
            if let (Some(ln), Some(rn)) = (parse_number(&left), parse_number(&right)) {
                return match comparer {
                    Comparer::EqualTo => (ln - rn).abs() < f64::EPSILON,
                    Comparer::NotEqualTo => (ln - rn).abs() >= f64::EPSILON,
                    Comparer::GreaterThan => ln > rn,
                    Comparer::GreaterOrEqual => ln >= rn,
                    Comparer::LessThan => ln < rn,
                    Comparer::LessThanOrEqual => ln <= rn,
                    _ => false,
                };
            }
            // String fallback
            match comparer {
                Comparer::EqualTo => left == right,
                Comparer::NotEqualTo => left != right,
                _ => false,
            }
        }
    }
}

/// Parse number: accepts comma as decimal separator, en-US style.
fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    t.parse::<f64>().ok()
}
