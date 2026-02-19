use rand::{RngExt, rng};

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

pub fn generate_password(length: usize) -> String {
    let mut rng = rng();
    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_password() {
        let password = generate_password(20);
        assert_eq!(password.len(), 20);
        assert!(password.chars().all(|c| {
            let c = c as u8;
            CHARSET.contains(&c)
        }));
    }
}
