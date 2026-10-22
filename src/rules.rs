// SPDX-License-Identifier: GPL-3.0-or-later

//! Rules for the administrator name and for passwords. The backend checks them again; these
//! checks only give the user an answer on the spot.

/// What is wrong with a name or a password. The screens turn it into a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    NameForm,
    NameReserved,
    PasswordTooShort,
    PasswordTooLong,
    PasswordTooCommon,
    PasswordCharacters,
    PasswordsDiffer,
    PasswordIsName,
}

pub const MIN_PASSWORD: usize = 12;
pub const MAX_PASSWORD: usize = 64;
/// Names the system uses, besides the ones the backend reports.
const ALWAYS_RESERVED: &[&str] = &["root"];

/// Lower-case letter first, then letters, digits, `-` and `_`; at most 32 characters.
pub fn check_name(name: &str, reserved: &[String]) -> Result<(), Problem> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_ok =
        chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !first_ok || !rest_ok || name.len() > 32 {
        return Err(Problem::NameForm);
    }
    if ALWAYS_RESERVED.contains(&name) || reserved.iter().any(|r| r == name) {
        return Err(Problem::NameReserved);
    }
    Ok(())
}

/// One rule for every password: printable ASCII (the startup screen uses a US keyboard layout),
/// 12 to 64 characters, not a well-known weak one.
pub fn check_password(pw: &str) -> Result<(), Problem> {
    if !pw.chars().all(|c| (' '..='~').contains(&c)) {
        return Err(Problem::PasswordCharacters);
    }
    if pw.len() < MIN_PASSWORD {
        return Err(Problem::PasswordTooShort);
    }
    if pw.len() > MAX_PASSWORD {
        return Err(Problem::PasswordTooLong);
    }
    if is_common(pw) {
        return Err(Problem::PasswordTooCommon);
    }
    Ok(())
}

/// Passwords of 12 characters or more that appear in every list of leaked passwords, and
/// passwords made of one repeated or counting pattern. Only entries of 12 characters or more
/// matter: shorter ones are refused by the length rule.
const COMMON: &[&str] = &[
    "password1234",
    "password12345",
    "password123456",
    "passw0rd1234",
    "qwertyuiop12",
    "qwertyuiop123",
    "qwertyuiopasdf",
    "qwertyuiopasdfghjkl",
    "qwertyuiop[]",
    "asdfghjkl123",
    "asdfghjklqwerty",
    "zxcvbnm12345",
    "iloveyou1234",
    "iloveyou12345",
    "letmein12345",
    "welcome12345",
    "administrator",
    "administrator1",
    "administrator123",
    "adminadmin123",
    "changeme1234",
    "trustno1trustno1",
    "1q2w3e4r5t6y",
    "1qaz2wsx3edc",
    "1q2w3e4r5t6y7u8i",
    "1qaz2wsx3edc4rfv",
    "123456789012",
    "1234567890123",
    "12345678901234",
    "123456789abc",
    "abcdefghijkl",
    "abcdefghijklmnop",
    "abcd12345678",
    "000000000000",
    "111111111111",
    "123123123123",
    "121212121212",
    "monkey123456",
    "dragon123456",
    "football1234",
    "baseball1234",
    "superman1234",
    "princess1234",
    "sunshine1234",
    "starwars1234",
    "whatever1234",
    "mypassword12",
    "mypassword123",
    "thisisapassword",
    "correcthorsebatterystaple",
    "nyraosnyraos",
    "nyra1234nyra",
];

fn is_common(pw: &str) -> bool {
    let lower = pw.to_ascii_lowercase();
    if COMMON.contains(&lower.as_str()) {
        return true;
    }
    let b = lower.as_bytes();
    // One character repeated, or a short block repeated ("abcabcabcabc").
    (1..=4).any(|n| b.len() % n == 0 && b.chunks(n).all(|c| c == &b[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let reserved = vec!["daemon".to_string()];
        assert_eq!(check_name("maria", &reserved), Ok(()));
        assert_eq!(check_name("a_b-9", &reserved), Ok(()));
        for bad in [
            "",
            "Maria",
            "1maria",
            "ma ria",
            "maria!",
            "é",
            &"a".repeat(33),
        ] {
            assert_eq!(
                check_name(bad, &reserved),
                Err(Problem::NameForm),
                "{bad:?}"
            );
        }
        assert_eq!(check_name(&"a".repeat(32), &reserved), Ok(()));
        assert_eq!(check_name("root", &reserved), Err(Problem::NameReserved));
        assert_eq!(check_name("daemon", &reserved), Err(Problem::NameReserved));
    }

    #[test]
    fn passwords() {
        assert_eq!(check_password("correct horse 42"), Ok(()));
        assert_eq!(check_password("short"), Err(Problem::PasswordTooShort));
        assert_eq!(
            check_password(&"x1".repeat(33)),
            Err(Problem::PasswordTooLong)
        );
        assert_eq!(
            check_password("Password12345"),
            Err(Problem::PasswordTooCommon)
        );
        assert_eq!(
            check_password("aaaaaaaaaaaa"),
            Err(Problem::PasswordTooCommon)
        );
        assert_eq!(
            check_password("abcabcabcabc"),
            Err(Problem::PasswordTooCommon)
        );
        assert_eq!(
            check_password("pässwörd-longer"),
            Err(Problem::PasswordCharacters)
        );
        assert_eq!(
            check_password("tab\there-long-1"),
            Err(Problem::PasswordCharacters)
        );
    }
}
