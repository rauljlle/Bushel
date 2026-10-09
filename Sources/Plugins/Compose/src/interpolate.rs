// Copyright © 2026 Apple Inc. and the container project authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::{bail, Result};
use std::collections::HashMap;

pub fn interpolate(input: &str, environment: &HashMap<String, String>) -> Result<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' {
            output.push(chars[index]);
            index += 1;
            continue;
        }
        if chars.get(index + 1) == Some(&'$') {
            output.push('$');
            index += 2;
            continue;
        }
        if chars.get(index + 1) != Some(&'{') {
            output.push('$');
            index += 1;
            continue;
        }
        let Some(relative_end) = chars[index + 2..]
            .iter()
            .position(|character| *character == '}')
        else {
            bail!("unterminated variable expression in Compose file");
        };
        let end = index + 2 + relative_end;
        let expression: String = chars[index + 2..end].iter().collect();
        output.push_str(&resolve(&expression, environment)?);
        index = end + 1;
    }
    Ok(output)
}

fn resolve(expression: &str, environment: &HashMap<String, String>) -> Result<String> {
    let operators = [":-", ":?", "-", "?"];
    let (name, operator, operand) = operators
        .iter()
        .find_map(|operator| {
            expression.find(operator).map(|position| {
                (
                    &expression[..position],
                    *operator,
                    &expression[position + operator.len()..],
                )
            })
        })
        .unwrap_or((expression, "", ""));

    if name.is_empty()
        || !name.chars().enumerate().all(|(offset, character)| {
            character == '_'
                || character.is_ascii_alphanumeric() && (offset > 0 || !character.is_ascii_digit())
        })
    {
        bail!("invalid variable expression: ${{{expression}}}");
    }

    let value = environment.get(name);
    let is_empty = value.map_or(true, String::is_empty);
    match operator {
        "" => Ok(value.cloned().unwrap_or_default()),
        ":-" if is_empty => Ok(operand.to_owned()),
        ":-" => Ok(value.cloned().unwrap_or_default()),
        "-" if value.is_none() => Ok(operand.to_owned()),
        "-" => Ok(value.cloned().unwrap_or_default()),
        ":?" if is_empty => bail!("{}", required_message(name, operand)),
        ":?" => Ok(value.cloned().unwrap_or_default()),
        "?" if value.is_none() => bail!("{}", required_message(name, operand)),
        "?" => Ok(value.cloned().unwrap_or_default()),
        _ => unreachable!(),
    }
}

fn required_message(name: &str, message: &str) -> String {
    if message.is_empty() {
        format!("environment variable {name} is required")
    } else {
        message.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_compose_forms() {
        let environment = HashMap::from([
            ("SET".to_owned(), "value".to_owned()),
            ("EMPTY".to_owned(), String::new()),
        ]);
        assert_eq!(
            interpolate(
                "${SET} ${MISSING:-fallback} ${EMPTY-default} $$SET",
                &environment
            )
            .unwrap(),
            "value fallback  $SET"
        );
    }

    #[test]
    fn rejects_missing_required_variable() {
        let error = interpolate("${TOKEN:?set TOKEN}", &HashMap::new()).unwrap_err();
        assert_eq!(error.to_string(), "set TOKEN");
    }
}
