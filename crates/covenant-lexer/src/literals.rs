//! Helpers for decoding literal token bodies.

/// Decode the body of an integer literal (underscore-allowed).
///
/// Returns `None` if the literal overflows `u128`.
pub fn parse_integer(slice: &str) -> Option<u128> {
    let mut cleaned = String::with_capacity(slice.len());
    for c in slice.chars() {
        if c != '_' {
            cleaned.push(c);
        }
    }
    cleaned.parse::<u128>().ok()
}

/// Result of decoding a hex literal body (the full slice including `0x`).
pub enum HexDecode {
    Ok(Box<[u8]>),
    /// `0x` with no digits after stripping underscores.
    Empty,
    /// Odd digit count after stripping underscores.
    Odd,
}

pub fn parse_hex(slice: &str) -> HexDecode {
    debug_assert!(slice.starts_with("0x") || slice.starts_with("0X"));
    let digits: String = slice[2..].chars().filter(|c| *c != '_').collect();
    if digits.is_empty() {
        return HexDecode::Empty;
    }
    if !digits.len().is_multiple_of(2) {
        return HexDecode::Odd;
    }
    let mut out = Vec::with_capacity(digits.len() / 2);
    let bytes = digits.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = hex_nibble(bytes[i]);
        let lo = hex_nibble(bytes[i + 1]);
        match (hi, lo) {
            (Some(h), Some(l)) => out.push((h << 4) | l),
            _ => return HexDecode::Odd, // defensive; logos regex guarantees hex chars
        }
        i += 2;
    }
    HexDecode::Ok(out.into_boxed_slice())
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + (b - b'a')),
        b'A'..=b'F' => Some(10 + (b - b'A')),
        _ => None,
    }
}

/// Outcome of decoding a text-literal body (including the surrounding quotes).
pub enum TextDecode {
    Ok(Box<str>),
    /// A `\X` escape where `X` is not one of the supported escapes.
    /// Returns the byte offset of the backslash relative to `slice` start
    /// and the (unparsed) escape fragment (e.g. `"\\x"`).
    BadEscape {
        offset: usize,
        fragment: String,
    },
}

/// Decode a double-quoted text literal.
///
/// Supported escapes: `\"`, `\\`, `\n`, `\t`, `\u{XXXX}`.
pub fn parse_text(slice: &str) -> TextDecode {
    debug_assert!(slice.starts_with('"') && slice.ends_with('"') && slice.len() >= 2);
    let inner = &slice[1..slice.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if i + 1 >= bytes.len() {
                return TextDecode::BadEscape {
                    offset: 1 + i,
                    fragment: "\\".to_string(),
                };
            }
            let esc = bytes[i + 1];
            match esc {
                b'"' => {
                    out.push('"');
                    i += 2;
                }
                b'\\' => {
                    out.push('\\');
                    i += 2;
                }
                b'n' => {
                    out.push('\n');
                    i += 2;
                }
                b't' => {
                    out.push('\t');
                    i += 2;
                }
                b'u' => {
                    // \u{XXXX}
                    if i + 2 >= bytes.len() || bytes[i + 2] != b'{' {
                        return TextDecode::BadEscape {
                            offset: 1 + i,
                            fragment: format!(
                                "\\u{}",
                                bytes.get(i + 2).map_or('?', |b| *b as char)
                            ),
                        };
                    }
                    // Find closing `}`.
                    let mut j = i + 3;
                    while j < bytes.len() && bytes[j] != b'}' {
                        j += 1;
                    }
                    if j >= bytes.len() {
                        return TextDecode::BadEscape {
                            offset: 1 + i,
                            fragment: "\\u{...".to_string(),
                        };
                    }
                    let hex = &inner[i + 3..j];
                    let cp = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                    match cp {
                        Some(ch) => {
                            out.push(ch);
                            i = j + 1;
                        }
                        None => {
                            return TextDecode::BadEscape {
                                offset: 1 + i,
                                fragment: format!("\\u{{{hex}}}"),
                            };
                        }
                    }
                }
                _ => {
                    return TextDecode::BadEscape {
                        offset: 1 + i,
                        fragment: format!("\\{}", esc as char),
                    };
                }
            }
        } else {
            // Copy the next UTF-8 code point. Use char iteration starting at byte i.
            let rest = &inner[i..];
            let ch = rest.chars().next().expect("non-empty slice has a char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    TextDecode::Ok(out.into_boxed_str())
}
