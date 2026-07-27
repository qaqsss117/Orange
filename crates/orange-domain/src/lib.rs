#![forbid(unsafe_code)]

pub const DOMAIN_SCHEMA_VERSION: u16 = 1;

#[cfg(test)]
mod tests {
    use super::DOMAIN_SCHEMA_VERSION;

    #[test]
    fn schema_version_starts_at_one() {
        assert_eq!(DOMAIN_SCHEMA_VERSION, 1);
    }
}
