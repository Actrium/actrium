#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordStrengthLevel {
    Weak,
    Fair,
    Good,
    Strong,
}

impl PasswordStrengthLevel {
    pub fn meter_level(self) -> u32 {
        match self {
            Self::Weak => 1,
            Self::Fair => 2,
            Self::Good => 3,
            Self::Strong => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Weak => "Weak",
            Self::Fair => "Fair",
            Self::Good => "Good",
            Self::Strong => "Strong",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordStrength {
    pub level: PasswordStrengthLevel,
    pub meter_level: u32,
    pub score: u32,
    pub label: String,
}

pub fn evaluate_password_strength(password: &str) -> PasswordStrength {
    let mut score = 0;
    let length = password.chars().count();

    if length >= 5 {
        score += 1;
    }
    if length >= 10 {
        score += 1;
    }
    if has_uppercase(password) && has_lowercase(password) {
        score += 1;
    }
    if password.chars().any(char::is_numeric) {
        score += 1;
    }
    if password.chars().any(is_punctuation_or_symbol) {
        score += 1;
    }

    let level = match score {
        0 | 1 => PasswordStrengthLevel::Weak,
        2 => PasswordStrengthLevel::Fair,
        3 => PasswordStrengthLevel::Good,
        _ => PasswordStrengthLevel::Strong,
    };

    PasswordStrength {
        level,
        meter_level: level.meter_level(),
        score,
        label: level.label().to_string(),
    }
}

fn has_uppercase(password: &str) -> bool {
    password.chars().any(char::is_uppercase)
}

fn has_lowercase(password: &str) -> bool {
    password.chars().any(char::is_lowercase)
}

fn is_punctuation_or_symbol(character: char) -> bool {
    !character.is_alphanumeric() && !character.is_whitespace()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_strength_matches_current_swift_thresholds() {
        assert_eq!(
            evaluate_password_strength("abcd").level,
            PasswordStrengthLevel::Weak
        );
        assert_eq!(
            evaluate_password_strength("abcde").level,
            PasswordStrengthLevel::Weak
        );
        assert_eq!(
            evaluate_password_strength("abcdefghij").level,
            PasswordStrengthLevel::Fair
        );
        assert_eq!(
            evaluate_password_strength("abcdefghij1").level,
            PasswordStrengthLevel::Good
        );
        assert_eq!(
            evaluate_password_strength("Abcdefghi1!").level,
            PasswordStrengthLevel::Strong
        );
    }

    #[test]
    fn strength_meter_level_matches_existing_ui() {
        let strength = evaluate_password_strength("Abcdefghi1!");
        assert_eq!(strength.meter_level, 4);
        assert_eq!(strength.label, "Strong");
        assert_eq!(strength.score, 5);
    }
}
